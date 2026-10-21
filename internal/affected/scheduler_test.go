package affected

import (
	"encoding/json"
	"os"
	"reflect"
	"testing"

	p "github.com/XionWCFM/nanoom/internal/prediction"
	w "github.com/XionWCFM/nanoom/internal/workspace"
)

func TestSchedulerMatchesRustOracle(t *testing.T) {
	data, err := os.ReadFile("testdata/rust-scheduler.json")
	if err != nil {
		t.Fatal(err)
	}
	var scenarios []struct {
		Mode        string               `json:"mode"`
		Context     p.PredictionContext  `json:"context"`
		Table       p.PredictionTable    `json:"table"`
		Preparation p.PreparationContext `json:"preparation"`
		Now         uint64               `json:"now"`
		Items       []json.RawMessage    `json:"items"`
		Matrix      json.RawMessage      `json:"matrix"`
		Labels      *[]string            `json:"labels"`
	}
	if err := json.Unmarshal(data, &scenarios); err != nil {
		t.Fatal(err)
	}
	for _, c := range scenarios {
		t.Run(c.Mode, func(t *testing.T) {
			entries := []Entry{}
			for _, raw := range c.Items {
				var item Entry
				if err := json.Unmarshal(raw, &item); err != nil {
					t.Fatal(err)
				}
				var extra struct {
					CheckoutPaths []string `json:"checkoutPaths"`
				}
				if err := json.Unmarshal(raw, &extra); err != nil {
					t.Fatal(err)
				}
				item.CheckoutPaths = extra.CheckoutPaths
				entries = append(entries, item)
			}
			index, err := p.NewIndex(nil, []p.PredictionTable{c.Table})
			if err != nil {
				t.Fatal(err)
			}
			tier := SelectTier(w.Distribution{Small: w.Tier{MaxAffectedPercent: 20, Concurrency: 1, RunnerLabels: c.Labels}, Medium: w.Tier{MaxAffectedPercent: 60, Concurrency: 2, RunnerLabels: c.Labels}, Full: w.Tier{MaxAffectedPercent: 100, Concurrency: 4, RunnerLabels: c.Labels}}, 100)
			output := Output{HasChange: true, Group: map[string]GroupOutput{"ci": {Label: "ci", Workspaces: entries, TotalWorkspaces: 4, AffectedWorkspaces: 4, AffectedPercent: 100, Distribution: &tier}}}
			actual := Matrix(output, index, &c.Context, "npm", "env", &c.Preparation, c.Now)
			bytes, err := json.Marshal(actual)
			if err != nil {
				t.Fatal(err)
			}
			var got, want any
			if err := json.Unmarshal(bytes, &got); err != nil {
				t.Fatal(err)
			}
			if err := json.Unmarshal(c.Matrix, &want); err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(got, want) {
				t.Fatalf("scheduler differs from Rust\ngot:%s\nwant:%s", bytes, c.Matrix)
			}
		})
	}
}
