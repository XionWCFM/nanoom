package cli

import (
	"encoding/json"
	"fmt"
	"io"
	"strings"
)

func status(a arguments, out io.Writer) error {
	if err := a.allow("json", "results"); err != nil {
		return err
	}
	if err := a.require("results"); err != nil {
		return err
	}
	if len(a.positionals) != 1 {
		return fmt.Errorf("status requires comma-separated job names")
	}
	type job struct {
		Name       string  `json:"name"`
		Status     string  `json:"status"`
		DurationMs *uint64 `json:"duration_ms"`
		URL        *string `json:"url"`
	}
	jobs := []job{}
	failed := false
	for _, name := range strings.Split(a.positionals[0], ",") {
		name = strings.TrimSpace(name)
		var statuses []string
		for _, entry := range strings.Split(a.get("results"), ",") {
			key, value, ok := strings.Cut(strings.TrimSpace(entry), "=")
			if ok && strings.TrimSpace(key) == name {
				statuses = append(statuses, strings.TrimSpace(value))
			}
		}
		if len(statuses) == 0 {
			return fmt.Errorf("status aggregation failed: missing result for job '%s'", name)
		}
		if len(statuses) > 1 {
			return fmt.Errorf("status aggregation failed: duplicate result for job '%s'", name)
		}
		switch statuses[0] {
		case "failure", "cancelled":
			failed = true
		case "success", "skipped":
		default:
			return fmt.Errorf("status aggregation failed: invalid status '%s' for job '%s'", statuses[0], name)
		}
		jobs = append(jobs, job{Name: name, Status: statuses[0]})
	}
	overall, reason := "success", "no job failed or was cancelled"
	if failed {
		overall = "failure"
		reason = "one or more jobs failed or were cancelled"
	}
	if a.has("json") {
		if err := json.NewEncoder(out).Encode(map[string]any{"jobs": jobs, "overall": overall, "reason": reason}); err != nil {
			return err
		}
	} else {
		fmt.Fprintln(out, "◆ nanoom status\n  Observed jobs:")
		for _, j := range jobs {
			emoji := "⏭️"
			if j.Status == "success" {
				emoji = "✅"
			}
			if j.Status == "failure" {
				emoji = "❌"
			}
			fmt.Fprintf(out, "  - %s: %s\n", j.Name, emoji)
		}
		fmt.Fprintf(out, "  Result: %s\n  Why: %s\n", strings.ToUpper(overall), reason)
	}
	if failed {
		return reportedFailure
	}
	return nil
}
