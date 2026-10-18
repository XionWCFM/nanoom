package cli

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestBatchArgumentHelper(t *testing.T) {
	if os.Getenv("NANOOM_BATCH_HELPER") != "1" {
		return
	}
	for i, arg := range os.Args {
		if arg == "--" {
			_ = json.NewEncoder(os.Stdout).Encode(os.Args[i+1:])
			os.Exit(0)
		}
	}
	os.Exit(2)
}

func TestWindowsBatchToolPreservesArguments(t *testing.T) {
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	directory := filepath.Join(t.TempDir(), "tool path with spaces")
	if err := os.MkdirAll(directory, 0755); err != nil {
		t.Fatal(err)
	}
	script := filepath.Join(directory, "tool.cmd")
	body := "@echo off\r\n\"" + executable + "\" -test.run=^TestBatchArgumentHelper$ -- %*\r\n"
	if err := os.WriteFile(script, []byte(body), 0644); err != nil {
		t.Fatal(err)
	}
	arguments := []string{"plain", "two words", "", "100%", "%PATH%", "!PATH!", "a&b", "a|b", "a<b", "a>b", "(a)", `a"b`, `C:\trailing\`}
	command, err := toolCommand(script, arguments)
	if err != nil {
		t.Fatal(err)
	}
	command.Env = append(os.Environ(), "NANOOM_BATCH_HELPER=1")
	output, err := command.CombinedOutput()
	if err != nil {
		t.Fatalf("batch failed: %v %s", err, output)
	}
	var actual []string
	if err := json.Unmarshal(output, &actual); err != nil {
		t.Fatalf("invalid child output: %v %s", err, output)
	}
	if !reflect.DeepEqual(actual, arguments) {
		t.Fatalf("arguments changed: got %#v want %#v", actual, arguments)
	}
}
