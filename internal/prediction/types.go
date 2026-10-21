package prediction

type Scope struct {
	RepositoryKey     string   `json:"repositoryKey"`
	WorkflowPath      string   `json:"workflowPath"`
	Ref               ScopeRef `json:"ref"`
	Group             string   `json:"group"`
	TaskRunner        string   `json:"taskRunner"`
	TimingEnvironment string   `json:"timingEnvironment"`
}

type PredictionContext struct {
	RepositoryKey string   `json:"repositoryKey"`
	WorkflowPath  string   `json:"workflowPath"`
	Ref           ScopeRef `json:"ref"`
}

type MeasurementArtifact struct {
	Version                 uint8                    `json:"version"`
	Scope                   Scope                    `json:"scope"`
	RunID                   string                   `json:"runId"`
	RunAttempt              uint32                   `json:"runAttempt"`
	Observations            []SuccessfulObservation  `json:"observations" optional:"true"`
	PreparationObservations []PreparationObservation `json:"preparationObservations,omitempty" optional:"true"`
	RunnerEnvironment       *RunnerEnvironment       `json:"runnerEnvironment,omitempty" optional:"true"`
}

type RunnerEnvironment struct {
	Fingerprint string        `json:"fingerprint"`
	Profile     RunnerProfile `json:"profile"`
}

type RunnerProfile struct {
	Version               uint8   `json:"version"`
	Os                    string  `json:"os"`
	Arch                  string  `json:"arch"`
	CPUModel              string  `json:"cpuModel"`
	AvailableCPUs         uint64  `json:"availableCpus"`
	MemoryMiB             uint64  `json:"memoryMiB"`
	CPUQuotaMilli         *uint64 `json:"cpuQuotaMilli"`
	MemoryLimitMiB        *uint64 `json:"memoryLimitMiB"`
	ContainerLimits       string  `json:"containerLimits"`
	Image                 string  `json:"image"`
	NodeVersion           string  `json:"nodeVersion"`
	PackageManager        string  `json:"packageManager"`
	PackageManagerVersion string  `json:"packageManagerVersion"`
}

type ArtifactReference struct {
	Name   string `json:"name"`
	Sha256 string `json:"sha256"`
}

type PredictionArtifact struct {
	Table         PredictionTable   `json:"table"`
	ModelArtifact ArtifactReference `json:"modelArtifact"`
}

type PredictionArtifactBundle struct {
	Version     uint8                `json:"version"`
	Predictions []PredictionArtifact `json:"predictions"`
}

type PredictionTable struct {
	Version                uint8                   `json:"version"`
	Scope                  Scope                   `json:"scope"`
	ModelUpdatedAtMs       uint64                  `json:"modelUpdatedAtMs"`
	Rows                   []PredictionRow         `json:"rows"`
	EnvironmentPredictions []EnvironmentPrediction `json:"environmentPredictions,omitempty" optional:"true"`
}

type EnvironmentPrediction struct {
	Environment RunnerEnvironment `json:"environment"`
	Table       *PredictionTable  `json:"table"`
}

type SuccessfulObservation struct {
	ExecutionID       string  `json:"executionId"`
	ObservedAtMs      uint64  `json:"observedAtMs"`
	Group             string  `json:"group"`
	Workspace         string  `json:"workspace"`
	Task              string  `json:"task"`
	Shard             *uint64 `json:"shard"`
	TotalShards       *uint64 `json:"totalShards"`
	TaskRunner        string  `json:"taskRunner"`
	TimingEnvironment string  `json:"timingEnvironment"`
	DurationMs        uint64  `json:"durationMs"`
}

type PreparationObservation struct {
	ExecutionID           string `json:"executionId"`
	ObservedAtMs          uint64 `json:"observedAtMs"`
	PackageManager        string `json:"packageManager"`
	PackageManagerVersion string `json:"packageManagerVersion"`
	InstallMode           string `json:"installMode"`
	LockfileDigest        string `json:"lockfileDigest"`
	CheckoutDigest        string `json:"checkoutDigest"`
	WorkspaceSetDigest    string `json:"workspaceSetDigest"`
	DurationMs            uint64 `json:"durationMs"`
}

type PreparationContext struct {
	PackageManager        string `json:"packageManager"`
	PackageManagerVersion string `json:"packageManagerVersion"`
	InstallMode           string `json:"installMode"`
	LockfileDigest        string `json:"lockfileDigest"`
}

type AggregateObservation struct {
	KeyID            string `json:"keyId"`
	UtcEpochDay      uint64 `json:"utcEpochDay"`
	ObservationCount uint64 `json:"observationCount"`
	TotalDurationMs  uint64 `json:"totalDurationMs"`
	LastObservedAtMs uint64 `json:"lastObservedAtMs"`
}

type ObservationBatch struct {
	Version            uint8                  `json:"version"`
	Scope              Scope                  `json:"scope"`
	BatchID            string                 `json:"batchId"`
	RunID              string                 `json:"runId"`
	RunAttempt         uint32                 `json:"runAttempt"`
	ProducedAtMs       uint64                 `json:"producedAtMs"`
	Aggregates         []AggregateObservation `json:"aggregates"`
	EnvironmentBatches []EnvironmentBatch     `json:"environmentBatches,omitempty" optional:"true"`
}

type EnvironmentBatch struct {
	Environment RunnerEnvironment `json:"environment"`
	Batch       *ObservationBatch `json:"batch"`
}

type ModelState struct {
	Version                uint8              `json:"version"`
	Scope                  Scope              `json:"scope"`
	UpdatedAtMs            uint64             `json:"updatedAtMs"`
	PruningDay             uint64             `json:"pruningDay"`
	BatchAcceptanceAfterMs uint64             `json:"batchAcceptanceAfterMs"`
	Entries                []ModelEntry       `json:"entries"`
	Receipts               []BatchReceipt     `json:"receipts"`
	EnvironmentStates      []EnvironmentState `json:"environmentStates,omitempty" optional:"true"`
}

type EnvironmentState struct {
	Environment RunnerEnvironment `json:"environment"`
	State       *ModelState       `json:"state"`
}

type ModelStateBundle struct {
	Version uint8        `json:"version"`
	States  []ModelState `json:"states"`
}

type ModelEntry struct {
	KeyID         string          `json:"keyId"`
	Buckets       []DayBucket     `json:"buckets"`
	RecentBatches []RecentBatch   `json:"recentBatches,omitempty" optional:"true"`
	Prediction    ModelPrediction `json:"prediction"`
}

// ScopeRef is serialized as the existing tagged push/pull_request union.
type ScopeRef struct {
	Kind             string `json:"kind"`
	Ref              string `json:"ref,omitempty"`
	Number           uint64 `json:"number,omitempty"`
	HeadRepositoryID string `json:"headRepositoryId,omitempty"`
	HeadRef          string `json:"headRef,omitempty"`
	BaseRef          string `json:"baseRef,omitempty"`
}

type PredictionRow struct {
	KeyID                                                         string
	EstimatedMs, ObservationCount, LastObservedAtMs, ValidUntilMs uint64
}
type RecentBatch struct {
	ObservedAtMs uint64
	RunID        string
	RunAttempt   uint32
	MeanMs       uint64
}
type BatchReceipt struct {
	BatchID, BodyDigest string
	ProducedAtMs        uint64
}
type DayBucket [4]uint64
type ModelPrediction [4]uint64
