package cli

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
)

func displayCommand(program string, args []string) string {
	parts := []string{strconv.Quote(program)}
	for _, arg := range args {
		parts = append(parts, strconv.Quote(arg))
	}
	return strings.Join(parts, " ")
}
func nodeTool(program string) string {
	if runtime.GOOS == "windows" {
		switch program {
		case "npm", "pnpm", "yarn", "turbo", "nx":
			return program + ".cmd"
		}
	}
	return program
}
func executeTool(program string, args []string, cwd string, environment map[string]string, jsonOutput bool, out, diagnostics io.Writer) error {
	executable := nodeTool(program)
	if program == "turbo" || program == "nx" {
		local := filepath.Join(cwd, "node_modules", ".bin", executable)
		if isFile(local) {
			executable = local
		}
		if environment == nil {
			environment = map[string]string{}
		}
		environment["PATH"] = filepath.Join(cwd, "node_modules", ".bin") + string(os.PathListSeparator) + os.Getenv("PATH")
	}
	cmd, err := toolCommand(executable, args)
	if err != nil {
		return err
	}
	cmd.Dir = cwd
	cmd.Env = os.Environ()
	for name, value := range environment {
		prefix := name + "="
		env := []string{}
		for _, entry := range cmd.Env {
			if !strings.HasPrefix(entry, prefix) {
				env = append(env, entry)
			}
		}
		cmd.Env = append(env, prefix+value)
	}
	if !jsonOutput {
		cmd.Stdout = out
		cmd.Stderr = diagnostics
		return cmd.Run()
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return err
	}
	if err := cmd.Start(); err != nil {
		return err
	}
	var lock sync.Mutex
	var wg sync.WaitGroup
	var streamError error
	forward := func(reader io.Reader) {
		defer wg.Done()
		buffer := bufio.NewReader(reader)
		for {
			line, err := buffer.ReadString('\n')
			if len(line) > 0 {
				line = strings.TrimSuffix(line, "\n")
				line = strings.TrimSuffix(line, "\r")
				if line != "::endgroup::" {
					if title, ok := strings.CutPrefix(line, "::group::"); ok {
						line = "◆ " + title
					}
					lock.Lock()
					_, writeErr := fmt.Fprintln(diagnostics, line)
					if writeErr != nil && streamError == nil {
						streamError = writeErr
					}
					lock.Unlock()
				}
			}
			if err != nil {
				if err != io.EOF {
					lock.Lock()
					if streamError == nil {
						streamError = err
					}
					lock.Unlock()
				}
				return
			}
		}
	}
	wg.Add(2)
	go forward(stdout)
	go forward(stderr)
	wg.Wait()
	status := cmd.Wait()
	if streamError != nil {
		return streamError
	}
	return status
}
