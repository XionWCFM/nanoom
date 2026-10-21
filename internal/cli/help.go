package cli

import (
	"embed"
	"fmt"
	"io"
	"strings"
)

//go:embed help/*.txt
var helpText embed.FS

func printHelp(positionals []string, version string, out io.Writer) error {
	name := "root"
	if len(positionals) > 0 {
		name = positionals[0]
	}
	if len(positionals) > 1 && name == "plan" {
		name += "-" + positionals[1]
	}
	// Read only known command names, never a client-provided file path.
	switch name {
	case "root", "affected", "run", "install", "history", "plan", "plan-select", "status", "schema", "cache-key", "version":
	default:
		return fmt.Errorf("unrecognized command %q", name)
	}
	data, err := helpText.ReadFile("help/" + name + ".txt")
	if err != nil {
		return err
	}
	_, err = io.WriteString(out, strings.ReplaceAll(string(data), "{{VERSION}}", version))
	return err
}
