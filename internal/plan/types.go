package plan

type PlanContext struct {
	Repository         string          `json:"repository"`
	Workflow           string          `json:"workflow"`
	RunID              string          `json:"runId"`
	ProducerAttempt    uint64          `json:"producerAttempt"`
	PlanningJob        string          `json:"planningJob"`
	Base               string          `json:"base"`
	Head               string          `json:"head"`
	TaskRunner         string          `json:"taskRunner"`
	PredictionReason   string          `json:"predictionReason" optional:"true"`
	PredictionArtifact *ArtifactDigest `json:"predictionArtifact,omitempty" optional:"true"`
	ModelArtifact      *ArtifactDigest `json:"modelArtifact,omitempty" optional:"true"`
}

type ArtifactDigest struct {
	Name   string `json:"name"`
	SHA256 string `json:"sha256"`
}

type PlanProvenance struct {
	Repository      string `json:"repository"`
	Workflow        string `json:"workflow"`
	RunID           string `json:"runId"`
	ProducerAttempt uint64 `json:"producerAttempt"`
	PlanningJob     string `json:"planningJob"`
	Base            string `json:"base"`
	Head            string `json:"head"`
}

type CurrentRun struct {
	Repository string `json:"repository"`
	Workflow   string `json:"workflow"`
	RunID      string `json:"runId"`
	Attempt    uint64 `json:"attempt"`
	Base       string `json:"base"`
	Head       string `json:"head"`
}

type PlanReference struct {
	Version      uint32         `json:"version"`
	ArtifactName string         `json:"artifactName"`
	SHA256       string         `json:"sha256"`
	Provenance   PlanProvenance `json:"provenance"`
	Current      CurrentRun     `json:"current"`
}

type Plan struct {
	Version            uint32               `json:"version"`
	Provenance         PlanProvenance       `json:"provenance"`
	TaskRunner         string               `json:"taskRunner"`
	ConfigPath         *string              `json:"configPath,omitempty" optional:"true"`
	WorkingDirectory   *string              `json:"workingDirectory,omitempty" optional:"true"`
	PredictionReason   string               `json:"predictionReason"`
	PredictionArtifact *ArtifactDigest      `json:"predictionArtifact,omitempty"`
	ModelArtifact      *ArtifactDigest      `json:"modelArtifact,omitempty"`
	HasChange          bool                 `json:"hasChange"`
	Groups             map[string]PlanGroup `json:"groups"`
	AssignmentCount    uint64               `json:"assignmentCount"`
	ItemCount          uint64               `json:"itemCount"`
}

type PlanGroup struct {
	Assignments []PlanAssignment `json:"assignments"`
}

type PlanAssignment struct {
	AssignmentID                string             `json:"assignmentId"`
	Items                       []PlanItem         `json:"items"`
	CheckoutPaths               []string           `json:"checkoutPaths"`
	PredictedDurationMs         *uint64            `json:"predictedDurationMs,omitempty"`
	PredictedPreparationMs      *uint64            `json:"predictedPreparationMs,omitempty"`
	PredictionSources           *PredictionSources `json:"predictionSources,omitempty"`
	PreparationPredictionSource *string            `json:"preparationPredictionSource,omitempty"`
	PreparationSampleCount      *uint64            `json:"preparationSampleCount,omitempty"`
	SchedulingMode              *string            `json:"schedulingMode,omitempty"`
	PredictionReason            string             `json:"predictionReason"`
	RunnerLabels                *[]string          `json:"runnerLabels,omitempty"`
	TimingEnvironment           *string            `json:"timingEnvironment,omitempty"`
}

type PlanItem struct {
	Group       string  `json:"group"`
	Name        string  `json:"name"`
	Path        string  `json:"path"`
	Task        string  `json:"task"`
	Shard       *uint64 `json:"shard,omitempty"`
	TotalShards *uint64 `json:"totalShards,omitempty"`
}

type AssignmentContext struct {
	Version                     uint32         `json:"version"`
	PlanSHA256                  string         `json:"planSha256"`
	Provenance                  PlanProvenance `json:"provenance"`
	Current                     CurrentRun     `json:"current"`
	TaskRunner                  string         `json:"taskRunner"`
	ConfigPath                  *string        `json:"configPath,omitempty" optional:"true"`
	WorkingDirectory            *string        `json:"workingDirectory,omitempty" optional:"true"`
	Group                       string         `json:"group"`
	AssignmentID                string         `json:"assignmentId"`
	Items                       []PlanItem     `json:"items"`
	CheckoutPaths               []string       `json:"checkoutPaths"`
	PredictedDurationMs         *uint64        `json:"predictedDurationMs,omitempty"`
	PredictedPreparationMs      *uint64        `json:"predictedPreparationMs,omitempty"`
	PredictionReason            string         `json:"predictionReason"`
	PreparationPredictionSource *string        `json:"preparationPredictionSource,omitempty"`
	PreparationSampleCount      *uint64        `json:"preparationSampleCount,omitempty"`
	SchedulingMode              *string        `json:"schedulingMode,omitempty"`
	RunnerLabels                *[]string      `json:"runnerLabels,omitempty"`
	TimingEnvironment           *string        `json:"timingEnvironment,omitempty"`
}

type PredictionSources struct {
	Exact                  uint64                  `json:"exact"`
	Group                  uint64                  `json:"group"`
	Cold                   uint64                  `json:"cold"`
	SampleCount            uint64                  `json:"sampleCount"`
	EnvironmentUncertainty *EnvironmentUncertainty `json:"environmentUncertainty,omitempty"`
}
type EnvironmentUncertainty struct {
	TaskCount        uint64 `json:"taskCount"`
	MinimumTaskSumMs uint64 `json:"minimumTaskSumMs"`
	MaximumTaskSumMs uint64 `json:"maximumTaskSumMs"`
}
