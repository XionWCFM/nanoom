package main

import (
	"github.com/XionWCFM/nanoom/internal/cli"
	"os"
)

// Version is replaced at release build time with -ldflags=-X=main.Version=...
var Version = "0.10.0"

func main() { os.Exit(cli.Main(os.Args[1:], os.Stdout, os.Stderr, Version)) }
