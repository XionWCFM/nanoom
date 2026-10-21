//go:build !windows

package cli

import "os/exec"

func toolCommand(program string, args []string) (*exec.Cmd, error) {
	return exec.Command(program, args...), nil
}
