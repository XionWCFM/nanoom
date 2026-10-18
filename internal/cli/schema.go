package cli

import (
	"fmt"
	"io"
	"os"

	"github.com/XionWCFM/nanoom"
)

func schemaCommand(a arguments, out io.Writer) error {
	if err := a.allow("output"); err != nil {
		return err
	}
	if len(a.positionals) > 0 {
		return fmt.Errorf("unexpected schema positional argument")
	}
	if a.has("output") {
		if err := os.WriteFile(a.get("output"), nanoom.ConfigSchema, 0644); err != nil {
			return err
		}
		_, err := fmt.Fprintf(out, "Schema written to %s\n", a.get("output"))
		return err
	}
	_, err := out.Write(nanoom.ConfigSchema)
	return err
}
