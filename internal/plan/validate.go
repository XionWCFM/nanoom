package plan

import (
	"fmt"
	"reflect"
	"regexp"
	"strings"
	"unicode"

	"github.com/XionWCFM/nanoom/internal/prediction"
)

const Version = 1

var shaPattern = regexp.MustCompile(`^[a-fA-F0-9]{40}$`)
var digestPattern = regexp.MustCompile(`^[a-fA-F0-9]{64}$`)
var decimalPattern = regexp.MustCompile(`^[0-9]+$`)
var jobPattern = regexp.MustCompile(`^[a-zA-Z0-9_.-]+$`)

func textValid(s string) bool {
	return strings.TrimSpace(s) != "" && strings.IndexFunc(s, unicode.IsControl) < 0
}
func ValidRelativePath(path string) bool {
	if path == "." {
		return true
	}
	if path == "" || strings.HasPrefix(path, "/") || strings.Contains(path, "\\") || strings.IndexFunc(path, unicode.IsControl) >= 0 || (len(path) >= 2 && ((path[0] >= 'a' && path[0] <= 'z') || (path[0] >= 'A' && path[0] <= 'Z')) && path[1] == ':') {
		return false
	}
	for _, part := range strings.Split(path, "/") {
		if part == "" || part == "." || part == ".." {
			return false
		}
	}
	return true
}
func (p PlanProvenance) Validate() error {
	for _, v := range []string{p.Repository, p.Workflow, p.RunID, p.PlanningJob} {
		if !textValid(v) {
			return fmt.Errorf("Plan provenance must be non-empty and have no controls")
		}
	}
	if !decimalPattern.MatchString(p.RunID) {
		return fmt.Errorf("Plan runId must contain decimal digits only")
	}
	if p.ProducerAttempt == 0 {
		return fmt.Errorf("Plan producerAttempt must be greater than zero")
	}
	if !jobPattern.MatchString(p.PlanningJob) {
		return fmt.Errorf("Plan planningJob has characters unsafe for an artifact name")
	}
	if !shaPattern.MatchString(p.Base) || !shaPattern.MatchString(p.Head) {
		return fmt.Errorf("Plan base and head must be full 40-character SHA-1 values")
	}
	return nil
}
func (a ArtifactDigest) Validate() error {
	if strings.TrimSpace(a.Name) == "" || !digestPattern.MatchString(a.SHA256) {
		return fmt.Errorf("Plan artifact reference needs a name and SHA-256 digest")
	}
	return nil
}
func (p Plan) Validate() error {
	if p.Version != Version {
		return fmt.Errorf("unsupported Plan version %d; expected 1", p.Version)
	}
	if err := p.Provenance.Validate(); err != nil {
		return err
	}
	if strings.TrimSpace(p.TaskRunner) == "" {
		return fmt.Errorf("Plan taskRunner must not be empty")
	}
	for _, path := range []*string{p.ConfigPath, p.WorkingDirectory} {
		if path != nil && (!ValidRelativePath(*path) || *path == ".") {
			return fmt.Errorf("Plan configuration and working directory must be repository-relative paths")
		}
	}
	for _, artifact := range []*ArtifactDigest{p.PredictionArtifact, p.ModelArtifact} {
		if artifact != nil {
			if err := artifact.Validate(); err != nil {
				return err
			}
		}
	}
	var assignments, items uint64
	identities := map[string]bool{}
	for name, group := range p.Groups {
		if strings.TrimSpace(name) == "" {
			return fmt.Errorf("Plan group names must not be empty")
		}
		seen := map[string]bool{}
		for _, a := range group.Assignments {
			assignments++
			if strings.TrimSpace(a.AssignmentID) == "" || seen[a.AssignmentID] {
				return fmt.Errorf("Plan group %q has an empty or duplicate assignmentId", name)
			}
			seen[a.AssignmentID] = true
			if len(a.Items) == 0 {
				return fmt.Errorf("Plan assignment %q has no items", a.AssignmentID)
			}
			if a.SchedulingMode != nil && *a.SchedulingMode != "automatic" && *a.SchedulingMode != "cold-cap" {
				return fmt.Errorf("Plan assignment has an unknown schedulingMode")
			}
			if a.PredictedPreparationMs != nil || a.PreparationPredictionSource != nil || a.PreparationSampleCount != nil {
				if a.PredictedPreparationMs == nil || a.PreparationPredictionSource == nil || a.PreparationSampleCount == nil || *a.PreparationSampleCount == 0 || (*a.PreparationPredictionSource != "exact" && *a.PreparationPredictionSource != "group") {
					return fmt.Errorf("Plan assignment has incomplete preparation prediction diagnostics")
				}
			}
			if len(a.CheckoutPaths) == 0 {
				return fmt.Errorf("Plan assignment has no checkout paths")
			}
			paths := map[string]bool{}
			previous := ""
			for _, path := range a.CheckoutPaths {
				if !ValidRelativePath(path) || path <= previous {
					return fmt.Errorf("Plan assignment has unsafe, duplicate, or unsorted checkout paths")
				}
				if p.WorkingDirectory != nil && path != *p.WorkingDirectory && !strings.HasPrefix(path, *p.WorkingDirectory+"/") {
					return fmt.Errorf("Plan checkout paths escape its workingDirectory")
				}
				previous = path
				paths[path] = true
			}
			for _, item := range a.Items {
				items++
				if item.Group != name || strings.TrimSpace(item.Name) == "" || strings.TrimSpace(item.Task) == "" || !ValidRelativePath(item.Path) {
					return fmt.Errorf("Plan item has invalid identity or path")
				}
				if !paths[item.Path] {
					return fmt.Errorf("Plan item path %q is not in checkout paths", item.Path)
				}
				identity, _ := prediction.Digest([]any{item.Group, item.Name, item.Task, item.Shard, item.TotalShards})
				if identities[identity] {
					return fmt.Errorf("Plan item %q is assigned more than once", item.Name)
				}
				identities[identity] = true
				if item.Shard != nil || item.TotalShards != nil {
					if item.Shard == nil || item.TotalShards == nil || *item.Shard == 0 || *item.Shard > *item.TotalShards {
						return fmt.Errorf("Plan item has an invalid shard layout")
					}
				}
			}
		}
	}
	if assignments != p.AssignmentCount || items != p.ItemCount {
		return fmt.Errorf("Plan assignment/item counts do not match its contents")
	}
	if p.HasChange != (items > 0) {
		return fmt.Errorf("Plan hasChange does not match its item count")
	}
	return nil
}
func (p PlanProvenance) ArtifactName() (string, error) {
	if err := p.Validate(); err != nil {
		return "", err
	}
	return fmt.Sprintf("nanoom-plan-v1-%s-%d-%s", p.RunID, p.ProducerAttempt, p.PlanningJob), nil
}
func (r PlanReference) Validate(p Plan, bytes []byte) error {
	if r.Version != Version {
		return fmt.Errorf("unsupported Plan reference version %d; expected 1", r.Version)
	}
	if err := r.Provenance.Validate(); err != nil {
		return err
	}
	if r.SHA256 != prediction.HexDigest(bytes) {
		return fmt.Errorf("Plan SHA-256 does not match its reference")
	}
	name, err := r.Provenance.ArtifactName()
	if err != nil {
		return err
	}
	if r.ArtifactName != name {
		return fmt.Errorf("Plan artifact name does not match its provenance")
	}
	c, provenance := r.Current, r.Provenance
	if c.Repository != provenance.Repository || c.Workflow != provenance.Workflow || c.RunID != provenance.RunID || !strings.EqualFold(c.Base, provenance.Base) || !strings.EqualFold(c.Head, provenance.Head) {
		return fmt.Errorf("Plan reference belongs to a different repository, workflow, run, or head")
	}
	if c.Attempt < provenance.ProducerAttempt {
		return fmt.Errorf("Plan producerAttempt is newer than the current run attempt")
	}
	if !reflect.DeepEqual(p.Provenance, provenance) {
		return fmt.Errorf("Plan provenance does not match its reference")
	}
	return p.Validate()
}
