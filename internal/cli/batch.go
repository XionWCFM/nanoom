package cli

import (
	"fmt"
	"strings"
)

// Escape both cmd.exe metacharacters and the child process's quoted argv.
// Reference: https://github.com/moxystudio/node-cross-spawn/blob/master/lib/util/escape.js
func batchCommand(value string) string {
	var out strings.Builder
	for _, c := range value {
		if strings.ContainsRune("()[]%!^\"`<>&|;, *?\t", c) {
			out.WriteByte('^')
		}
		out.WriteRune(c)
	}
	return out.String()
}
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
				out.WriteByte('\\')
			}
			slashes = 0
		}
		out.WriteRune(c)
	}
	out.WriteString(strings.Repeat("\\", slashes))
	out.WriteByte('"')
	return batchCommand(out.String()), nil
}
