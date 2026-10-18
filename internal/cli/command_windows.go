package cli

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
)

func toolCommand(program string, args []string) (*exec.Cmd, error) {
	resolved, err := exec.LookPath(program)
	if err != nil {
		return nil, err
	}
	extension := strings.ToLower(filepath.Ext(resolved))
	if extension != ".cmd" && extension != ".bat" {
		return exec.Command(resolved, args...), nil
	}
	script, err := filepath.Abs(resolved)
	if err != nil {
		return nil, err
	}
	script = strings.TrimPrefix(script, `\\?\`)
	if strings.ContainsRune(script, '"') {
		return nil, fmt.Errorf("invalid Windows batch path")
	}
	root := os.Getenv("SystemRoot")
	if root == "" {
		return nil, fmt.Errorf("SystemRoot is required to execute a Windows batch file")
	}
	interpreter := filepath.Join(root, "System32", "cmd.exe")
	line := `cmd.exe /e:ON /v:OFF /d /s /c "` + batchCommand(script)
	for _, arg := range args {
		quoted, err := batchArgument(arg)
		if err != nil {
			return nil, err
		}
		line += " " + quoted
	}
	line += `"`
	command := exec.Command(interpreter)
	command.SysProcAttr = &syscall.SysProcAttr{CmdLine: line}
	return command, nil
}
