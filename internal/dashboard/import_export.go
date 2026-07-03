package dashboard

import (
	"encoding/json"
	"fmt"
	"time"

	"github.com/onllm-dev/onwatch/v2/internal/store"
)

// ExportPackage represents the full export payload.
// These structs do not contain keys/tokens/cookies/passwords — only status,
// paths, BaseURLs, model info, quotas, and notes are included.
type ExportPackage struct {
	SchemaVersion     string                    `json:"schema_version"`
	ExportedAt        string                    `json:"exported_at"`
	Platforms         []store.Platform          `json:"platforms"`
	Plans             []store.Plan              `json:"plans"`
	QuotaBuckets      []store.QuotaBucket       `json:"quota_buckets"`
	Models            []store.Model             `json:"models"`
	UsageLogs         []store.UsageLog          `json:"usage_logs"`
	CredentialStatuses []store.CredentialStatus `json:"credential_statuses"`
	RiskNotes         []store.RiskNote          `json:"risk_notes"`
}

// ImportResult holds the count of successfully imported records per table.
type ImportResult struct {
	Platforms         int `json:"platforms"`
	Plans             int `json:"plans"`
	QuotaBuckets      int `json:"quota_buckets"`
	Models            int `json:"models"`
	UsageLogs         int `json:"usage_logs"`
	CredentialStatuses int `json:"credential_statuses"`
	RiskNotes         int `json:"risk_notes"`
}

// ExportData collects all quota-board data from the store and marshals it
// into indented JSON. No sensitive credentials are included — only metadata
// such as paths, BaseURLs, model info, quota values, and notes.
func ExportData(s *store.Store) ([]byte, error) {
	platforms, err := s.ListPlatforms()
	if err != nil {
		return nil, fmt.Errorf("export: list platforms: %w", err)
	}
	plans, err := s.ListPlans()
	if err != nil {
		return nil, fmt.Errorf("export: list plans: %w", err)
	}
	buckets, err := s.ListQuotaBuckets()
	if err != nil {
		return nil, fmt.Errorf("export: list quota buckets: %w", err)
	}
	models, err := s.ListModels()
	if err != nil {
		return nil, fmt.Errorf("export: list models: %w", err)
	}
	logs, err := s.ListUsageLogs()
	if err != nil {
		return nil, fmt.Errorf("export: list usage logs: %w", err)
	}
	creds, err := s.ListCredentialStatuses()
	if err != nil {
		return nil, fmt.Errorf("export: list credential statuses: %w", err)
	}
	notes, err := s.ListRiskNotes()
	if err != nil {
		return nil, fmt.Errorf("export: list risk notes: %w", err)
	}

	pkg := ExportPackage{
		SchemaVersion:     "1",
		ExportedAt:        time.Now().UTC().Format(time.RFC3339),
		Platforms:         platforms,
		Plans:             plans,
		QuotaBuckets:      buckets,
		Models:            models,
		UsageLogs:         logs,
		CredentialStatuses: creds,
		RiskNotes:         notes,
	}
	return json.MarshalIndent(pkg, "", "  ")
}

// ImportData unmarshals an export payload and inserts records into the store.
//
// mode controls the import strategy:
//   - "overwrite": deletes all existing records (in dependency-safe order),
//     then inserts from the export preserving original IDs.
//   - "upsert" (default): inserts every record with original IDs; if a record
//     conflicts (e.g. duplicate primary key), it is replaced.
func ImportData(s *store.Store, data []byte, mode string) (*ImportResult, error) {
	if mode == "" {
		mode = "upsert"
	}

	var pkg ExportPackage
	if err := json.Unmarshal(data, &pkg); err != nil {
		return nil, fmt.Errorf("import: unmarshal: %w", err)
	}

	result := &ImportResult{}

	if mode == "overwrite" {
		if err := s.DeleteAllQB(); err != nil {
			return nil, fmt.Errorf("import: delete all: %w", err)
		}
	}

	// Import in forward dependency order (parent first).
	for _, p := range pkg.Platforms {
		if err := s.InsertPlatformWithID(&p); err != nil {
			if mode == "overwrite" {
				return nil, fmt.Errorf("import: insert platform %q: %w", p.Name, err)
			}
			continue
		}
		result.Platforms++
	}
	for _, pl := range pkg.Plans {
		if err := s.InsertPlanWithID(&pl); err != nil {
			if mode == "overwrite" {
				return nil, fmt.Errorf("import: insert plan %q: %w", pl.Name, err)
			}
			continue
		}
		result.Plans++
	}
	for _, qb := range pkg.QuotaBuckets {
		if err := s.InsertQuotaBucketWithID(&qb); err != nil {
			if mode == "overwrite" {
				return nil, fmt.Errorf("import: insert quota bucket: %w", err)
			}
			continue
		}
		result.QuotaBuckets++
	}
	for _, m := range pkg.Models {
		if err := s.InsertModelWithID(&m); err != nil {
			if mode == "overwrite" {
				return nil, fmt.Errorf("import: insert model %q: %w", m.DisplayName, err)
			}
			continue
		}
		result.Models++
	}
	for _, ul := range pkg.UsageLogs {
		if err := s.InsertUsageLogWithID(&ul); err != nil {
			if mode == "overwrite" {
				return nil, fmt.Errorf("import: insert usage log: %w", err)
			}
			continue
		}
		result.UsageLogs++
	}
	for _, cs := range pkg.CredentialStatuses {
		if err := s.InsertCredentialStatusWithID(&cs); err != nil {
			if mode == "overwrite" {
				return nil, fmt.Errorf("import: insert credential status: %w", err)
			}
			continue
		}
		result.CredentialStatuses++
	}
	for _, rn := range pkg.RiskNotes {
		if err := s.InsertRiskNoteWithID(&rn); err != nil {
			if mode == "overwrite" {
				return nil, fmt.Errorf("import: insert risk note: %w", err)
			}
			continue
		}
		result.RiskNotes++
	}

	return result, nil
}