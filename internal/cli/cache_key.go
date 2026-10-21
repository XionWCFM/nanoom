package cli

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
)

func cacheKey(a arguments, cwd, config string, out, diagnostics io.Writer) error {
	if err := a.allow("runner", "task", "filter", "json"); err != nil {
		return err
	}
	if err := a.require("runner", "task"); err != nil {
		return err
	}
	if len(a.positionals) > 0 {
		return fmt.Errorf("unexpected cache-key positional argument")
	}
	stat, err := os.Stat(cwd)
	if err != nil {
		return err
	}
	if !stat.IsDir() {
		return fmt.Errorf("cache-key working directory must be a directory")
	}
	h := sha256.New()
	_, _ = h.Write([]byte("nanoom-cache-key-v2\x00"))
	for _, s := range []string{a.get("runner"), a.get("task"), a.get("filter")} {
		_, _ = h.Write([]byte(s))
		_, _ = h.Write([]byte{0})
	}
	files := []string{}
	for _, file := range []string{config, "package.json", "pnpm-workspace.yaml", ".npmrc", ".yarnrc.yml", "pnpm-lock.yaml", "yarn.lock", "package-lock.json", "npm-shrinkwrap.json"} {
		name := strings.ReplaceAll(file, "\\", "/")
		_, _ = h.Write([]byte(name))
		_, _ = h.Write([]byte{0})
		path := file
		if !filepath.IsAbs(file) {
			path = filepath.Join(cwd, file)
		}
		b, err := os.ReadFile(path)
		if os.IsNotExist(err) {
			_, _ = h.Write([]byte{0})
			continue
		}
		if err != nil {
			return err
		}
		_, _ = h.Write([]byte{1})
		var length [8]byte
		binary.LittleEndian.PutUint64(length[:], uint64(len(b)))
		_, _ = h.Write(length[:])
		_, _ = h.Write(b)
		files = append(files, name)
	}
	digest := hex.EncodeToString(h.Sum(nil))
	key := fmt.Sprintf("nanoom-%s-%s-%s", a.get("runner"), a.get("task"), digest[:16])
	if a.has("json") {
		return json.NewEncoder(out).Encode(map[string]any{"key": key, "runner": a.get("runner"), "task": a.get("task"), "filter": a.get("filter"), "cwd": cwd, "hashedFiles": files, "reason": "SHA-256 over runner, task, filter, configuration, manifests, and supported lockfiles"})
	}
	filter := a.get("filter")
	if filter == "" {
		filter = "<none>"
	}
	fmt.Fprintf(diagnostics, "◆ nanoom cache-key\n  Inputs: runner=%s task=%s filter=%s files=%s\n", a.get("runner"), a.get("task"), filter, strings.Join(files, ","))
	_, err = fmt.Fprintln(out, key)
	return err
}
