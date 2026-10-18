package cli

import (
	"fmt"
	"strings"
)

// Batch quoting follows Rust std's cmd.exe boundary: disable delayed expansion,
// double quotes/backslashes, and interrupt percent-variable expansion.
// https://github.com/rust-lang/rust/blob/master/library/std/src/sys/args/windows.rs
func batchArgument(value string) (string, error) {
	if strings.ContainsAny(value, "\x00\r\n") {
		return "", fmt.Errorf("batch file arguments are invalid")
	}
	var out strings.Builder
	out.WriteByte('"')
	slashes := 0
	for _, c := range value {
		if c == '\\' {
			slashes++
		} else {
			if c == '"' {
				out.WriteString(strings.Repeat("\\", slashes))
				out.WriteByte('"')
			} else if c == '%' {
				out.WriteString("%%cd:~,")
			}
			slashes = 0
		}
		out.WriteRune(c)
	}
	out.WriteString(strings.Repeat("\\", slashes))
	out.WriteByte('"')
	return out.String(), nil
}
