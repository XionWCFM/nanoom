package cli

import (
	"encoding/json"
	"fmt"
	"io"
	"path/filepath"

	p "github.com/XionWCFM/nanoom/internal/plan"
)

func planCommand(a arguments, cwd string, out io.Writer) error {
	if err := a.allow("input", "reference", "group", "assignment", "output-dir"); err != nil {
		return err
	}
	if err := a.require("input", "reference", "group", "assignment", "output-dir"); err != nil {
		return err
	}
	if len(a.positionals) != 1 || a.positionals[0] != "select" {
		return fmt.Errorf("plan requires the select subcommand")
	}
	resolve := func(path string) string {
		if filepath.IsAbs(path) {
			return path
		}
		return filepath.Join(cwd, path)
	}
	output := resolve(a.get("output-dir"))
	context, err := p.Select(resolve(a.get("input")), resolve(a.get("reference")), a.get("group"), a.get("assignment"), output)
	if err != nil {
		return err
	}
	return json.NewEncoder(out).Encode(map[string]any{"status": "success", "group": context.Group, "assignmentId": context.AssignmentID, "itemCount": len(context.Items), "checkoutPathCount": len(context.CheckoutPaths), "assignmentFile": filepath.Join(output, "assignment.json"), "pathsFile": filepath.Join(output, "paths.txt")})
}
