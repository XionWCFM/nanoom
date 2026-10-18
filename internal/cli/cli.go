package cli

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
)

type arguments struct {
	options     map[string][]string
	positionals []string
	repeat      map[string]bool
}

var booleans = map[string]bool{"json": true, "help": true, "version": true, "verbose": true, "all": true, "continue-on-error": true}

func parse(args []string) (arguments, error) {
	a := arguments{options: map[string][]string{}}
	for i := 0; i < len(args); i++ {
		item := args[i]
		if item == "--" {
			a.positionals = append(a.positionals, args[i+1:]...)
			break
		}
		if !strings.HasPrefix(item, "-") {
			a.positionals = append(a.positionals, item)
			continue
		}
		if strings.HasPrefix(item, "-") && !strings.HasPrefix(item, "--") && strings.Trim(item[1:], "v") == "" {
			for range item[1:] {
				a.options["verbose"] = append(a.options["verbose"], "true")
			}
			continue
		}
		if len(item) > 2 && (strings.HasPrefix(item, "-C") || strings.HasPrefix(item, "-c")) {
			name := "config"
			if item[1] == 'C' {
				name = "cwd"
			}
			a.options[name] = append(a.options[name], strings.TrimPrefix(item[2:], "="))
			continue
		}
		name := strings.TrimPrefix(item, "--")
		if item == "-C" {
			name = "cwd"
		}
		if item == "-c" {
			name = "config"
		}
		if item == "-v" {
			name = "verbose"
		}
		if item == "-h" {
			name = "help"
		}
		if item == "-V" {
			name = "version"
		}
		name, value, hasValue := strings.Cut(name, "=")
		if booleans[name] {
			if hasValue {
				return a, fmt.Errorf("unexpected value for --%s", name)
			}
			value = "true"
		} else if !hasValue {
			if i+1 == len(args) || strings.HasPrefix(args[i+1], "--") {
				return a, fmt.Errorf("--%s requires a value", name)
			}
			i++
			value = args[i]
		}
		a.options[name] = append(a.options[name], value)
	}
	return a, nil
}
func (a arguments) get(name string) string {
	values := a.options[name]
	if len(values) > 0 {
		return values[len(values)-1]
	}
	return ""
}
func (a arguments) has(name string) bool { return len(a.options[name]) > 0 }
func (a arguments) require(names ...string) error {
	for _, name := range names {
		if !a.has(name) {
			return fmt.Errorf("--%s is required", name)
		}
	}
	return nil
}
func (a arguments) allow(names ...string) error {
	allowed := map[string]bool{"config": true, "cwd": true, "verbose": true, "help": true}
	for _, name := range names {
		allowed[name] = true
	}
	for name, values := range a.options {
		if !allowed[name] {
			return fmt.Errorf("unexpected argument '--%s'", name)
		}
		if len(values) > 1 && !a.repeat[name] {
			return fmt.Errorf("--%s may only be supplied once", name)
		}
	}
	return nil
}

var reportedFailure = errors.New("one or more jobs did not succeed")

func Main(args []string, stdout, stderr io.Writer, version string) int {
	a, err := parse(args)
	helpPositionals := a.positionals
	explicitHelp := false
	if len(helpPositionals) > 0 && helpPositionals[0] == "help" {
		explicitHelp = true
		helpPositionals = helpPositionals[1:]
	}
	if len(helpPositionals) > 1 && helpPositionals[0] == "plan" && helpPositionals[1] == "help" {
		explicitHelp = true
		helpPositionals = append([]string{"plan"}, helpPositionals[2:]...)
	}
	if err == nil && (a.has("help") || explicitHelp) {
		if helpErr := printHelp(helpPositionals, version, stdout); helpErr == nil {
			return 0
		} else {
			err = helpErr
		}
	}
	if err == nil && a.has("version") {
		fmt.Fprintln(stdout, "nanoom", version)
		return 0
	}
	if err == nil {
		err = dispatch(a, stdout, stderr, version)
	}
	if err == nil {
		return 0
	}
	if a.has("json") && !errors.Is(err, reportedFailure) {
		_ = json.NewEncoder(stdout).Encode(map[string]any{"status": "failure", "error": err.Error()})
	}
	fmt.Fprintln(stderr, "error:", err)
	return 1
}
func dispatch(a arguments, out, diagnostics io.Writer, version string) error {
	if len(a.positionals) == 0 {
		return fmt.Errorf("a command is required")
	}
	a.repeat = map[string]bool{"verbose": true}
	command := a.positionals[0]
	if command == "history" {
		a.repeat["input"] = true
	}
	if command == "affected" {
		a.repeat["prediction"] = true
		a.repeat["prediction-table"] = true
	}
	if command == "install" {
		a.repeat["filter"] = true
	}
	a.positionals = a.positionals[1:]
	cwd := a.get("cwd")
	if cwd == "" {
		var err error
		cwd, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	config := a.get("config")
	if config == "" {
		config = "nanoom.config.json"
	}
	if a.has("verbose") {
		fmt.Fprintf(diagnostics, "nanoom: cwd=%s config=%s command=%s\n", cwd, config, command)
	}
	switch command {
	case "version":
		if err := a.allow("json"); err != nil {
			return err
		}
		if len(a.positionals) > 0 {
			return fmt.Errorf("unexpected argument %q", a.positionals[0])
		}
		if a.has("json") {
			return json.NewEncoder(out).Encode(map[string]string{"name": "nanoom", "version": version})
		}
		_, err := fmt.Fprintln(out, "nanoom", version)
		return err
	case "status":
		return status(a, out)
	case "cache-key":
		return cacheKey(a, cwd, config, out, diagnostics)
	case "history":
		return history(a, out, diagnostics)
	case "plan":
		return planCommand(a, cwd, out)
	case "install":
		return install(a, cwd, config, out, diagnostics)
	case "affected":
		return affected(a, cwd, config, out, diagnostics)
	case "run":
		return runTasks(a, cwd, config, out, diagnostics)
	case "schema":
		return schemaCommand(a, out)
	default:
		return fmt.Errorf("command %q has not been migrated to Go", command)
	}
}
func writeFile(path string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
		return err
	}
	return os.WriteFile(path, data, 0644)
}
