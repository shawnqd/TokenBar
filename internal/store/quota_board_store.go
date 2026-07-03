package store

import (
	"database/sql"
	"fmt"
	"time"
)

// ---------------------------------------------------------------------------
// Struct definitions
// ---------------------------------------------------------------------------

// Platform represents a model platform/provider.
type Platform struct {
	ID                int64
	Name              string
	Vendor            string
	Category          string
	BaseURL           string
	CredentialStatus  string
	SupportsToolsJSON bool
	DefaultRiskLevel  string
	IsActive          bool
	Notes             string
	CreatedAt         time.Time
	UpdatedAt         time.Time
}

// Plan represents a subscription plan belonging to a platform.
type Plan struct {
	ID              int64
	PlatformID      int64
	Name            string
	PlanType        string
	StartsAt        string
	ExpiresAt       string
	RenewalPolicy   string
	Status          string
	Priority        int
	RecommendedRole string
	RiskSummary     string
	CreatedAt       time.Time
	UpdatedAt       time.Time
}

// QuotaBucket represents a quota metric bucket under a plan.
type QuotaBucket struct {
	ID              int64
	PlanID          int64
	Scope           string
	Metric          string
	LimitValue      float64
	RemainingValue  float64
	UsedValue       float64
	WindowStart     string
	WindowEnd       string
	ResetAt         string
	Source          string
	ConfidenceLevel string
	Notes           string
	CreatedAt       time.Time
	UpdatedAt       time.Time
}

// Model represents a model linked to a platform and optionally a plan.
type Model struct {
	ID              int64
	PlatformID      int64
	PlanID          *int64
	ModelID         string
	DisplayName     string
	Family          string
	IsCurrent       bool
	BaseURLOverride string
	ToolFitJSON     string
	Status          string
	CreatedAt       time.Time
	UpdatedAt       time.Time
}

// UsageLog represents a recorded usage entry.
type UsageLog struct {
	ID              int64
	PlanID          *int64
	ModelID         *int64
	BucketScope     string
	DateKey         string
	PeriodStart     string
	PeriodEnd       string
	InputTokens     int64
	OutputTokens    int64
	CacheReadTokens int64
	CacheWriteTokens int64
	RequestCount    int64
	CostValue       float64
	Source          string
	SourceRef       string
	CreatedAt       time.Time
}

// CredentialStatus represents the credential check result for a platform.
type CredentialStatus struct {
	ID              int64
	PlatformID      int64
	Status          string
	DetectionMethod string
	DetectedPath    string
	CheckedAt       string
	MessageRedacted string
	BaseURL         string
}

// RiskNote represents a risk annotation attached to any target.
type RiskNote struct {
	ID         int64
	TargetType string
	TargetID   *int64
	Level      string
	Title      string
	Content    string
	Source     string
	ExpiresAt  string
	ResolvedAt string
	CreatedAt  time.Time
	UpdatedAt  time.Time
}

// UsageSummary contains aggregated usage totals for a plan within a period.
type UsageSummary struct {
	PlanID            int64
	InputTokens       int64
	OutputTokens      int64
	CacheReadTokens   int64
	CacheWriteTokens  int64
	RequestCount      int64
	CostValue         float64
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

func boolToInt(b bool) int64 {
	if b {
		return 1
	}
	return 0
}

func scanTime(s string) (time.Time, error) {
	return time.Parse(time.RFC3339Nano, s)
}

func mustParseTime(s string) time.Time {
	t, _ := scanTime(s)
	return t
}

// ---------------------------------------------------------------------------
// Platform CRUD
// ---------------------------------------------------------------------------

// InsertPlatform inserts a new platform row and returns its ID.
func (s *Store) InsertPlatform(p *Platform) (int64, error) {
	if p == nil {
		return 0, fmt.Errorf("platform is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	res, err := s.db.Exec(`
		INSERT INTO qb_platforms (
			name, vendor, category, base_url, credential_status,
			supports_tools_json, default_risk_level, is_active, notes,
			created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		p.Name, p.Vendor, p.Category, p.BaseURL, p.CredentialStatus,
		boolToInt(p.SupportsToolsJSON), p.DefaultRiskLevel, boolToInt(p.IsActive), p.Notes,
		now, now,
	)
	if err != nil {
		return 0, fmt.Errorf("failed to insert platform: %w", err)
	}
	id, err := res.LastInsertId()
	if err != nil {
		return 0, fmt.Errorf("failed to get platform id: %w", err)
	}
	return id, nil
}

// GetPlatform retrieves a platform by id.
func (s *Store) GetPlatform(id int64) (*Platform, error) {
	var p Platform
	var supportsToolsJSON, isActive int64
	var createdAt, updatedAt string
	err := s.db.QueryRow(`
		SELECT id, name, vendor, category, base_url, credential_status,
		       supports_tools_json, default_risk_level, is_active, notes,
		       created_at, updated_at
		FROM qb_platforms WHERE id = ?
	`, id).Scan(
		&p.ID, &p.Name, &p.Vendor, &p.Category, &p.BaseURL, &p.CredentialStatus,
		&supportsToolsJSON, &p.DefaultRiskLevel, &isActive, &p.Notes,
		&createdAt, &updatedAt,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to get platform %d: %w", id, err)
	}
	p.SupportsToolsJSON = supportsToolsJSON != 0
	p.IsActive = isActive != 0
	p.CreatedAt = mustParseTime(createdAt)
	p.UpdatedAt = mustParseTime(updatedAt)
	return &p, nil
}

// ListPlatforms returns all platforms.
func (s *Store) ListPlatforms() ([]Platform, error) {
	rows, err := s.db.Query(`
		SELECT id, name, vendor, category, base_url, credential_status,
		       supports_tools_json, default_risk_level, is_active, notes,
		       created_at, updated_at
		FROM qb_platforms ORDER BY name
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to list platforms: %w", err)
	}
	defer rows.Close()

	var list []Platform
	for rows.Next() {
		var p Platform
		var supportsToolsJSON, isActive int64
		var createdAt, updatedAt string
		if err := rows.Scan(
			&p.ID, &p.Name, &p.Vendor, &p.Category, &p.BaseURL, &p.CredentialStatus,
			&supportsToolsJSON, &p.DefaultRiskLevel, &isActive, &p.Notes,
			&createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan platform: %w", err)
		}
		p.SupportsToolsJSON = supportsToolsJSON != 0
		p.IsActive = isActive != 0
		p.CreatedAt = mustParseTime(createdAt)
		p.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, p)
	}
	return list, rows.Err()
}

// UpdatePlatform updates all mutable fields of a platform.
func (s *Store) UpdatePlatform(p *Platform) error {
	if p == nil {
		return fmt.Errorf("platform is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	_, err := s.db.Exec(`
		UPDATE qb_platforms SET
			name = ?, vendor = ?, category = ?, base_url = ?,
			credential_status = ?, supports_tools_json = ?, default_risk_level = ?,
			is_active = ?, notes = ?, updated_at = ?
		WHERE id = ?
	`,
		p.Name, p.Vendor, p.Category, p.BaseURL,
		p.CredentialStatus, boolToInt(p.SupportsToolsJSON), p.DefaultRiskLevel,
		boolToInt(p.IsActive), p.Notes, now,
		p.ID,
	)
	if err != nil {
		return fmt.Errorf("failed to update platform %d: %w", p.ID, err)
	}
	return nil
}

// DeletePlatform deletes a platform by id.
func (s *Store) DeletePlatform(id int64) error {
	_, err := s.db.Exec(`DELETE FROM qb_platforms WHERE id = ?`, id)
	if err != nil {
		return fmt.Errorf("failed to delete platform %d: %w", id, err)
	}
	return nil
}

// ---------------------------------------------------------------------------
// Plan CRUD + association
// ---------------------------------------------------------------------------

// InsertPlan inserts a new plan row and returns its ID.
func (s *Store) InsertPlan(pl *Plan) (int64, error) {
	if pl == nil {
		return 0, fmt.Errorf("plan is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	res, err := s.db.Exec(`
		INSERT INTO qb_plans (
			platform_id, name, plan_type, starts_at, expires_at,
			renewal_policy, status, priority, recommended_role, risk_summary,
			created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		pl.PlatformID, pl.Name, pl.PlanType, pl.StartsAt, pl.ExpiresAt,
		pl.RenewalPolicy, pl.Status, pl.Priority, pl.RecommendedRole, pl.RiskSummary,
		now, now,
	)
	if err != nil {
		return 0, fmt.Errorf("failed to insert plan: %w", err)
	}
	id, err := res.LastInsertId()
	if err != nil {
		return 0, fmt.Errorf("failed to get plan id: %w", err)
	}
	return id, nil
}

// GetPlan retrieves a plan by id.
func (s *Store) GetPlan(id int64) (*Plan, error) {
	var pl Plan
	var createdAt, updatedAt string
	err := s.db.QueryRow(`
		SELECT id, platform_id, name, plan_type, starts_at, expires_at,
		       renewal_policy, status, priority, recommended_role, risk_summary,
		       created_at, updated_at
		FROM qb_plans WHERE id = ?
	`, id).Scan(
		&pl.ID, &pl.PlatformID, &pl.Name, &pl.PlanType, &pl.StartsAt, &pl.ExpiresAt,
		&pl.RenewalPolicy, &pl.Status, &pl.Priority, &pl.RecommendedRole, &pl.RiskSummary,
		&createdAt, &updatedAt,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to get plan %d: %w", id, err)
	}
	pl.CreatedAt = mustParseTime(createdAt)
	pl.UpdatedAt = mustParseTime(updatedAt)
	return &pl, nil
}

// ListPlans returns all plans.
func (s *Store) ListPlans() ([]Plan, error) {
	rows, err := s.db.Query(`
		SELECT id, platform_id, name, plan_type, starts_at, expires_at,
		       renewal_policy, status, priority, recommended_role, risk_summary,
		       created_at, updated_at
		FROM qb_plans ORDER BY name
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to list plans: %w", err)
	}
	defer rows.Close()

	var list []Plan
	for rows.Next() {
		var pl Plan
		var createdAt, updatedAt string
		if err := rows.Scan(
			&pl.ID, &pl.PlatformID, &pl.Name, &pl.PlanType, &pl.StartsAt, &pl.ExpiresAt,
			&pl.RenewalPolicy, &pl.Status, &pl.Priority, &pl.RecommendedRole, &pl.RiskSummary,
			&createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan plan: %w", err)
		}
		pl.CreatedAt = mustParseTime(createdAt)
		pl.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, pl)
	}
	return list, rows.Err()
}

// UpdatePlan updates all mutable fields of a plan.
func (s *Store) UpdatePlan(pl *Plan) error {
	if pl == nil {
		return fmt.Errorf("plan is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	_, err := s.db.Exec(`
		UPDATE qb_plans SET
			platform_id = ?, name = ?, plan_type = ?, starts_at = ?, expires_at = ?,
			renewal_policy = ?, status = ?, priority = ?, recommended_role = ?, risk_summary = ?,
			updated_at = ?
		WHERE id = ?
	`,
		pl.PlatformID, pl.Name, pl.PlanType, pl.StartsAt, pl.ExpiresAt,
		pl.RenewalPolicy, pl.Status, pl.Priority, pl.RecommendedRole, pl.RiskSummary,
		now, pl.ID,
	)
	if err != nil {
		return fmt.Errorf("failed to update plan %d: %w", pl.ID, err)
	}
	return nil
}

// DeletePlan deletes a plan by id.
func (s *Store) DeletePlan(id int64) error {
	_, err := s.db.Exec(`DELETE FROM qb_plans WHERE id = ?`, id)
	if err != nil {
		return fmt.Errorf("failed to delete plan %d: %w", id, err)
	}
	return nil
}

// ListPlansByPlatform returns all plans for a given platform.
func (s *Store) ListPlansByPlatform(platformID int64) ([]Plan, error) {
	rows, err := s.db.Query(`
		SELECT id, platform_id, name, plan_type, starts_at, expires_at,
		       renewal_policy, status, priority, recommended_role, risk_summary,
		       created_at, updated_at
		FROM qb_plans WHERE platform_id = ? ORDER BY priority DESC, name
	`, platformID)
	if err != nil {
		return nil, fmt.Errorf("failed to list plans by platform %d: %w", platformID, err)
	}
	defer rows.Close()

	var list []Plan
	for rows.Next() {
		var pl Plan
		var createdAt, updatedAt string
		if err := rows.Scan(
			&pl.ID, &pl.PlatformID, &pl.Name, &pl.PlanType, &pl.StartsAt, &pl.ExpiresAt,
			&pl.RenewalPolicy, &pl.Status, &pl.Priority, &pl.RecommendedRole, &pl.RiskSummary,
			&createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan plan: %w", err)
		}
		pl.CreatedAt = mustParseTime(createdAt)
		pl.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, pl)
	}
	return list, rows.Err()
}

// ListExpiringPlans returns plans whose expires_at is within the next N days.
func (s *Store) ListExpiringPlans(days int) ([]Plan, error) {
	now := time.Now().UTC()
	end := now.AddDate(0, 0, days)
	rows, err := s.db.Query(`
		SELECT id, platform_id, name, plan_type, starts_at, expires_at,
		       renewal_policy, status, priority, recommended_role, risk_summary,
		       created_at, updated_at
		FROM qb_plans
		WHERE expires_at != ''
		  AND expires_at >= ? AND expires_at < ?
		ORDER BY expires_at ASC
	`, now.Format(time.RFC3339Nano), end.Format(time.RFC3339Nano))
	if err != nil {
		return nil, fmt.Errorf("failed to list expiring plans: %w", err)
	}
	defer rows.Close()

	var list []Plan
	for rows.Next() {
		var pl Plan
		var createdAt, updatedAt string
		if err := rows.Scan(
			&pl.ID, &pl.PlatformID, &pl.Name, &pl.PlanType, &pl.StartsAt, &pl.ExpiresAt,
			&pl.RenewalPolicy, &pl.Status, &pl.Priority, &pl.RecommendedRole, &pl.RiskSummary,
			&createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan plan: %w", err)
		}
		pl.CreatedAt = mustParseTime(createdAt)
		pl.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, pl)
	}
	return list, rows.Err()
}

// ---------------------------------------------------------------------------
// QuotaBucket CRUD + association + derived
// ---------------------------------------------------------------------------

// InsertQuotaBucket inserts a new quota bucket row and returns its ID.
func (s *Store) InsertQuotaBucket(qb *QuotaBucket) (int64, error) {
	if qb == nil {
		return 0, fmt.Errorf("quota bucket is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	res, err := s.db.Exec(`
		INSERT INTO qb_quota_buckets (
			plan_id, scope, metric, limit_value, remaining_value, used_value,
			window_start, window_end, reset_at, source, confidence_level, notes,
			created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		qb.PlanID, qb.Scope, qb.Metric, qb.LimitValue, qb.RemainingValue, qb.UsedValue,
		qb.WindowStart, qb.WindowEnd, qb.ResetAt, qb.Source, qb.ConfidenceLevel, qb.Notes,
		now, now,
	)
	if err != nil {
		return 0, fmt.Errorf("failed to insert quota bucket: %w", err)
	}
	id, err := res.LastInsertId()
	if err != nil {
		return 0, fmt.Errorf("failed to get quota bucket id: %w", err)
	}
	return id, nil
}

// GetQuotaBucket retrieves a quota bucket by id.
func (s *Store) GetQuotaBucket(id int64) (*QuotaBucket, error) {
	var qb QuotaBucket
	var createdAt, updatedAt string
	err := s.db.QueryRow(`
		SELECT id, plan_id, scope, metric, limit_value, remaining_value, used_value,
		       window_start, window_end, reset_at, source, confidence_level, notes,
		       created_at, updated_at
		FROM qb_quota_buckets WHERE id = ?
	`, id).Scan(
		&qb.ID, &qb.PlanID, &qb.Scope, &qb.Metric, &qb.LimitValue, &qb.RemainingValue, &qb.UsedValue,
		&qb.WindowStart, &qb.WindowEnd, &qb.ResetAt, &qb.Source, &qb.ConfidenceLevel, &qb.Notes,
		&createdAt, &updatedAt,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to get quota bucket %d: %w", id, err)
	}
	qb.CreatedAt = mustParseTime(createdAt)
	qb.UpdatedAt = mustParseTime(updatedAt)
	return &qb, nil
}

// ListQuotaBuckets returns all quota buckets.
func (s *Store) ListQuotaBuckets() ([]QuotaBucket, error) {
	rows, err := s.db.Query(`
		SELECT id, plan_id, scope, metric, limit_value, remaining_value, used_value,
		       window_start, window_end, reset_at, source, confidence_level, notes,
		       created_at, updated_at
		FROM qb_quota_buckets ORDER BY plan_id, scope
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to list quota buckets: %w", err)
	}
	defer rows.Close()

	var list []QuotaBucket
	for rows.Next() {
		var qb QuotaBucket
		var createdAt, updatedAt string
		if err := rows.Scan(
			&qb.ID, &qb.PlanID, &qb.Scope, &qb.Metric, &qb.LimitValue, &qb.RemainingValue, &qb.UsedValue,
			&qb.WindowStart, &qb.WindowEnd, &qb.ResetAt, &qb.Source, &qb.ConfidenceLevel, &qb.Notes,
			&createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan quota bucket: %w", err)
		}
		qb.CreatedAt = mustParseTime(createdAt)
		qb.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, qb)
	}
	return list, rows.Err()
}

// UpdateQuotaBucket updates all mutable fields of a quota bucket.
func (s *Store) UpdateQuotaBucket(qb *QuotaBucket) error {
	if qb == nil {
		return fmt.Errorf("quota bucket is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	_, err := s.db.Exec(`
		UPDATE qb_quota_buckets SET
			plan_id = ?, scope = ?, metric = ?,
			limit_value = ?, remaining_value = ?, used_value = ?,
			window_start = ?, window_end = ?, reset_at = ?,
			source = ?, confidence_level = ?, notes = ?,
			updated_at = ?
		WHERE id = ?
	`,
		qb.PlanID, qb.Scope, qb.Metric,
		qb.LimitValue, qb.RemainingValue, qb.UsedValue,
		qb.WindowStart, qb.WindowEnd, qb.ResetAt,
		qb.Source, qb.ConfidenceLevel, qb.Notes,
		now, qb.ID,
	)
	if err != nil {
		return fmt.Errorf("failed to update quota bucket %d: %w", qb.ID, err)
	}
	return nil
}

// DeleteQuotaBucket deletes a quota bucket by id.
func (s *Store) DeleteQuotaBucket(id int64) error {
	_, err := s.db.Exec(`DELETE FROM qb_quota_buckets WHERE id = ?`, id)
	if err != nil {
		return fmt.Errorf("failed to delete quota bucket %d: %w", id, err)
	}
	return nil
}

// ListQuotaBucketsByPlan returns all quota buckets for a given plan.
func (s *Store) ListQuotaBucketsByPlan(planID int64) ([]QuotaBucket, error) {
	rows, err := s.db.Query(`
		SELECT id, plan_id, scope, metric, limit_value, remaining_value, used_value,
		       window_start, window_end, reset_at, source, confidence_level, notes,
		       created_at, updated_at
		FROM qb_quota_buckets WHERE plan_id = ? ORDER BY scope
	`, planID)
	if err != nil {
		return nil, fmt.Errorf("failed to list quota buckets by plan %d: %w", planID, err)
	}
	defer rows.Close()

	var list []QuotaBucket
	for rows.Next() {
		var qb QuotaBucket
		var createdAt, updatedAt string
		if err := rows.Scan(
			&qb.ID, &qb.PlanID, &qb.Scope, &qb.Metric, &qb.LimitValue, &qb.RemainingValue, &qb.UsedValue,
			&qb.WindowStart, &qb.WindowEnd, &qb.ResetAt, &qb.Source, &qb.ConfidenceLevel, &qb.Notes,
			&createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan quota bucket: %w", err)
		}
		qb.CreatedAt = mustParseTime(createdAt)
		qb.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, qb)
	}
	return list, rows.Err()
}

// ListLowQuotaBuckets returns buckets where remaining/limit drops below threshold.
func (s *Store) ListLowQuotaBuckets(thresholdRatio float64) ([]QuotaBucket, error) {
	rows, err := s.db.Query(`
		SELECT id, plan_id, scope, metric, limit_value, remaining_value, used_value,
		       window_start, window_end, reset_at, source, confidence_level, notes,
		       created_at, updated_at
		FROM qb_quota_buckets
		WHERE limit_value > 0 AND remaining_value / limit_value < ?
		ORDER BY remaining_value / limit_value ASC
	`, thresholdRatio)
	if err != nil {
		return nil, fmt.Errorf("failed to list low quota buckets: %w", err)
	}
	defer rows.Close()

	var list []QuotaBucket
	for rows.Next() {
		var qb QuotaBucket
		var createdAt, updatedAt string
		if err := rows.Scan(
			&qb.ID, &qb.PlanID, &qb.Scope, &qb.Metric, &qb.LimitValue, &qb.RemainingValue, &qb.UsedValue,
			&qb.WindowStart, &qb.WindowEnd, &qb.ResetAt, &qb.Source, &qb.ConfidenceLevel, &qb.Notes,
			&createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan quota bucket: %w", err)
		}
		qb.CreatedAt = mustParseTime(createdAt)
		qb.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, qb)
	}
	return list, rows.Err()
}

// ---------------------------------------------------------------------------
// Model CRUD + association
// ---------------------------------------------------------------------------

// InsertModel inserts a new model row and returns its ID.
func (s *Store) InsertModel(m *Model) (int64, error) {
	if m == nil {
		return 0, fmt.Errorf("model is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	res, err := s.db.Exec(`
		INSERT INTO qb_models (
			platform_id, plan_id, model_id, display_name, family,
			is_current, base_url_override, tool_fit_json, status,
			created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		m.PlatformID, m.PlanID, m.ModelID, m.DisplayName, m.Family,
		boolToInt(m.IsCurrent), m.BaseURLOverride, m.ToolFitJSON, m.Status,
		now, now,
	)
	if err != nil {
		return 0, fmt.Errorf("failed to insert model: %w", err)
	}
	id, err := res.LastInsertId()
	if err != nil {
		return 0, fmt.Errorf("failed to get model id: %w", err)
	}
	return id, nil
}

// GetModel retrieves a model by id.
func (s *Store) GetModel(id int64) (*Model, error) {
	var m Model
	var planID sql.NullInt64
	var isCurrent int64
	var createdAt, updatedAt string
	err := s.db.QueryRow(`
		SELECT id, platform_id, plan_id, model_id, display_name, family,
		       is_current, base_url_override, tool_fit_json, status,
		       created_at, updated_at
		FROM qb_models WHERE id = ?
	`, id).Scan(
		&m.ID, &m.PlatformID, &planID, &m.ModelID, &m.DisplayName, &m.Family,
		&isCurrent, &m.BaseURLOverride, &m.ToolFitJSON, &m.Status,
		&createdAt, &updatedAt,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to get model %d: %w", id, err)
	}
	if planID.Valid {
		m.PlanID = &planID.Int64
	}
	m.IsCurrent = isCurrent != 0
	m.CreatedAt = mustParseTime(createdAt)
	m.UpdatedAt = mustParseTime(updatedAt)
	return &m, nil
}

// ListModels returns all models.
func (s *Store) ListModels() ([]Model, error) {
	rows, err := s.db.Query(`
		SELECT id, platform_id, plan_id, model_id, display_name, family,
		       is_current, base_url_override, tool_fit_json, status,
		       created_at, updated_at
		FROM qb_models ORDER BY display_name
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to list models: %w", err)
	}
	defer rows.Close()

	var list []Model
	for rows.Next() {
		var m Model
		var planID sql.NullInt64
		var isCurrent int64
		var createdAt, updatedAt string
		if err := rows.Scan(
			&m.ID, &m.PlatformID, &planID, &m.ModelID, &m.DisplayName, &m.Family,
			&isCurrent, &m.BaseURLOverride, &m.ToolFitJSON, &m.Status,
			&createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan model: %w", err)
		}
		if planID.Valid {
			m.PlanID = &planID.Int64
		}
		m.IsCurrent = isCurrent != 0
		m.CreatedAt = mustParseTime(createdAt)
		m.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, m)
	}
	return list, rows.Err()
}

// UpdateModel updates all mutable fields of a model.
func (s *Store) UpdateModel(m *Model) error {
	if m == nil {
		return fmt.Errorf("model is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	_, err := s.db.Exec(`
		UPDATE qb_models SET
			platform_id = ?, plan_id = ?, model_id = ?, display_name = ?, family = ?,
			is_current = ?, base_url_override = ?, tool_fit_json = ?, status = ?,
			updated_at = ?
		WHERE id = ?
	`,
		m.PlatformID, m.PlanID, m.ModelID, m.DisplayName, m.Family,
		boolToInt(m.IsCurrent), m.BaseURLOverride, m.ToolFitJSON, m.Status,
		now, m.ID,
	)
	if err != nil {
		return fmt.Errorf("failed to update model %d: %w", m.ID, err)
	}
	return nil
}

// DeleteModel deletes a model by id.
func (s *Store) DeleteModel(id int64) error {
	_, err := s.db.Exec(`DELETE FROM qb_models WHERE id = ?`, id)
	if err != nil {
		return fmt.Errorf("failed to delete model %d: %w", id, err)
	}
	return nil
}

// ListModelsByPlatform returns all models for a given platform.
func (s *Store) ListModelsByPlatform(platformID int64) ([]Model, error) {
	rows, err := s.db.Query(`
		SELECT id, platform_id, plan_id, model_id, display_name, family,
		       is_current, base_url_override, tool_fit_json, status,
		       created_at, updated_at
		FROM qb_models WHERE platform_id = ? ORDER BY display_name
	`, platformID)
	if err != nil {
		return nil, fmt.Errorf("failed to list models by platform %d: %w", platformID, err)
	}
	defer rows.Close()

	var list []Model
	for rows.Next() {
		var m Model
		var planID sql.NullInt64
		var isCurrent int64
		var createdAt, updatedAt string
		if err := rows.Scan(
			&m.ID, &m.PlatformID, &planID, &m.ModelID, &m.DisplayName, &m.Family,
			&isCurrent, &m.BaseURLOverride, &m.ToolFitJSON, &m.Status,
			&createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan model: %w", err)
		}
		if planID.Valid {
			m.PlanID = &planID.Int64
		}
		m.IsCurrent = isCurrent != 0
		m.CreatedAt = mustParseTime(createdAt)
		m.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, m)
	}
	return list, rows.Err()
}

// ListModelsByPlan returns all models for a given plan.
func (s *Store) ListModelsByPlan(planID int64) ([]Model, error) {
	rows, err := s.db.Query(`
		SELECT id, platform_id, plan_id, model_id, display_name, family,
		       is_current, base_url_override, tool_fit_json, status,
		       created_at, updated_at
		FROM qb_models WHERE plan_id = ? ORDER BY display_name
	`, planID)
	if err != nil {
		return nil, fmt.Errorf("failed to list models by plan %d: %w", planID, err)
	}
	defer rows.Close()

	var list []Model
	for rows.Next() {
		var m Model
		var nullablePlanID sql.NullInt64
		var isCurrent int64
		var createdAt, updatedAt string
		if err := rows.Scan(
			&m.ID, &m.PlatformID, &nullablePlanID, &m.ModelID, &m.DisplayName, &m.Family,
			&isCurrent, &m.BaseURLOverride, &m.ToolFitJSON, &m.Status,
			&createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan model: %w", err)
		}
		if nullablePlanID.Valid {
			m.PlanID = &nullablePlanID.Int64
		}
		m.IsCurrent = isCurrent != 0
		m.CreatedAt = mustParseTime(createdAt)
		m.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, m)
	}
	return list, rows.Err()
}

// ---------------------------------------------------------------------------
// UsageLog CRUD + derived
// ---------------------------------------------------------------------------

// InsertUsageLog inserts a new usage log row and returns its ID.
func (s *Store) InsertUsageLog(ul *UsageLog) (int64, error) {
	if ul == nil {
		return 0, fmt.Errorf("usage log is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	res, err := s.db.Exec(`
		INSERT INTO qb_usage_logs (
			plan_id, model_id, bucket_scope, date_key,
			period_start, period_end,
			input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
			request_count, cost_value, source, source_ref, created_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		ul.PlanID, ul.ModelID, ul.BucketScope, ul.DateKey,
		ul.PeriodStart, ul.PeriodEnd,
		ul.InputTokens, ul.OutputTokens, ul.CacheReadTokens, ul.CacheWriteTokens,
		ul.RequestCount, ul.CostValue, ul.Source, ul.SourceRef, now,
	)
	if err != nil {
		return 0, fmt.Errorf("failed to insert usage log: %w", err)
	}
	id, err := res.LastInsertId()
	if err != nil {
		return 0, fmt.Errorf("failed to get usage log id: %w", err)
	}
	return id, nil
}

// GetUsageLog retrieves a usage log by id.
func (s *Store) GetUsageLog(id int64) (*UsageLog, error) {
	var ul UsageLog
	var planID, modelID sql.NullInt64
	var createdAt string
	err := s.db.QueryRow(`
		SELECT id, plan_id, model_id, bucket_scope, date_key,
		       period_start, period_end,
		       input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
		       request_count, cost_value, source, source_ref, created_at
		FROM qb_usage_logs WHERE id = ?
	`, id).Scan(
		&ul.ID, &planID, &modelID, &ul.BucketScope, &ul.DateKey,
		&ul.PeriodStart, &ul.PeriodEnd,
		&ul.InputTokens, &ul.OutputTokens, &ul.CacheReadTokens, &ul.CacheWriteTokens,
		&ul.RequestCount, &ul.CostValue, &ul.Source, &ul.SourceRef, &createdAt,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to get usage log %d: %w", id, err)
	}
	if planID.Valid {
		ul.PlanID = &planID.Int64
	}
	if modelID.Valid {
		ul.ModelID = &modelID.Int64
	}
	ul.CreatedAt = mustParseTime(createdAt)
	return &ul, nil
}

// ListUsageLogs returns all usage logs.
func (s *Store) ListUsageLogs() ([]UsageLog, error) {
	rows, err := s.db.Query(`
		SELECT id, plan_id, model_id, bucket_scope, date_key,
		       period_start, period_end,
		       input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
		       request_count, cost_value, source, source_ref, created_at
		FROM qb_usage_logs ORDER BY created_at
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to list usage logs: %w", err)
	}
	defer rows.Close()

	var list []UsageLog
	for rows.Next() {
		var ul UsageLog
		var planID, modelID sql.NullInt64
		var createdAt string
		if err := rows.Scan(
			&ul.ID, &planID, &modelID, &ul.BucketScope, &ul.DateKey,
			&ul.PeriodStart, &ul.PeriodEnd,
			&ul.InputTokens, &ul.OutputTokens, &ul.CacheReadTokens, &ul.CacheWriteTokens,
			&ul.RequestCount, &ul.CostValue, &ul.Source, &ul.SourceRef, &createdAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan usage log: %w", err)
		}
		if planID.Valid {
			ul.PlanID = &planID.Int64
		}
		if modelID.Valid {
			ul.ModelID = &modelID.Int64
		}
		ul.CreatedAt = mustParseTime(createdAt)
		list = append(list, ul)
	}
	return list, rows.Err()
}

// UpdateUsageLog updates mutable fields of a usage log.
func (s *Store) UpdateUsageLog(ul *UsageLog) error {
	if ul == nil {
		return fmt.Errorf("usage log is nil")
	}
	_, err := s.db.Exec(`
		UPDATE qb_usage_logs SET
			plan_id = ?, model_id = ?, bucket_scope = ?, date_key = ?,
			period_start = ?, period_end = ?,
			input_tokens = ?, output_tokens = ?, cache_read_tokens = ?, cache_write_tokens = ?,
			request_count = ?, cost_value = ?, source = ?, source_ref = ?
		WHERE id = ?
	`,
		ul.PlanID, ul.ModelID, ul.BucketScope, ul.DateKey,
		ul.PeriodStart, ul.PeriodEnd,
		ul.InputTokens, ul.OutputTokens, ul.CacheReadTokens, ul.CacheWriteTokens,
		ul.RequestCount, ul.CostValue, ul.Source, ul.SourceRef,
		ul.ID,
	)
	if err != nil {
		return fmt.Errorf("failed to update usage log %d: %w", ul.ID, err)
	}
	return nil
}

// DeleteUsageLog deletes a usage log by id.
func (s *Store) DeleteUsageLog(id int64) error {
	_, err := s.db.Exec(`DELETE FROM qb_usage_logs WHERE id = ?`, id)
	if err != nil {
		return fmt.Errorf("failed to delete usage log %d: %w", id, err)
	}
	return nil
}

// UsageSummaryByPeriod returns aggregated usage totals for a plan within a period.
func (s *Store) UsageSummaryByPeriod(planID int64, periodStart, periodEnd string) (UsageSummary, error) {
	var summary UsageSummary
	summary.PlanID = planID
	err := s.db.QueryRow(`
		SELECT
			COALESCE(SUM(input_tokens), 0),
			COALESCE(SUM(output_tokens), 0),
			COALESCE(SUM(cache_read_tokens), 0),
			COALESCE(SUM(cache_write_tokens), 0),
			COALESCE(SUM(request_count), 0),
			COALESCE(SUM(cost_value), 0)
		FROM qb_usage_logs
		WHERE plan_id = ? AND period_start >= ? AND period_end <= ?
	`, planID, periodStart, periodEnd).Scan(
		&summary.InputTokens,
		&summary.OutputTokens,
		&summary.CacheReadTokens,
		&summary.CacheWriteTokens,
		&summary.RequestCount,
		&summary.CostValue,
	)
	if err != nil {
		return summary, fmt.Errorf("failed to query usage summary for plan %d: %w", planID, err)
	}
	return summary, nil
}

// ---------------------------------------------------------------------------
// CredentialStatus CRUD + association
// ---------------------------------------------------------------------------

// InsertCredentialStatus inserts a new credential status row and returns its ID.
func (s *Store) InsertCredentialStatus(cs *CredentialStatus) (int64, error) {
	if cs == nil {
		return 0, fmt.Errorf("credential status is nil")
	}
	res, err := s.db.Exec(`
		INSERT INTO qb_credential_statuses (
			platform_id, status, detection_method, detected_path,
			checked_at, message_redacted, base_url
		) VALUES (?, ?, ?, ?, ?, ?, ?)
	`,
		cs.PlatformID, cs.Status, cs.DetectionMethod, cs.DetectedPath,
		cs.CheckedAt, cs.MessageRedacted, cs.BaseURL,
	)
	if err != nil {
		return 0, fmt.Errorf("failed to insert credential status: %w", err)
	}
	id, err := res.LastInsertId()
	if err != nil {
		return 0, fmt.Errorf("failed to get credential status id: %w", err)
	}
	return id, nil
}

// GetCredentialStatus retrieves a credential status by id.
func (s *Store) GetCredentialStatus(id int64) (*CredentialStatus, error) {
	var cs CredentialStatus
	err := s.db.QueryRow(`
		SELECT id, platform_id, status, detection_method, detected_path,
		       checked_at, message_redacted, base_url
		FROM qb_credential_statuses WHERE id = ?
	`, id).Scan(
		&cs.ID, &cs.PlatformID, &cs.Status, &cs.DetectionMethod, &cs.DetectedPath,
		&cs.CheckedAt, &cs.MessageRedacted, &cs.BaseURL,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to get credential status %d: %w", id, err)
	}
	return &cs, nil
}

// ListCredentialStatuses returns all credential statuses.
func (s *Store) ListCredentialStatuses() ([]CredentialStatus, error) {
	rows, err := s.db.Query(`
		SELECT id, platform_id, status, detection_method, detected_path,
		       checked_at, message_redacted, base_url
		FROM qb_credential_statuses ORDER BY platform_id
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to list credential statuses: %w", err)
	}
	defer rows.Close()

	var list []CredentialStatus
	for rows.Next() {
		var cs CredentialStatus
		if err := rows.Scan(
			&cs.ID, &cs.PlatformID, &cs.Status, &cs.DetectionMethod, &cs.DetectedPath,
			&cs.CheckedAt, &cs.MessageRedacted, &cs.BaseURL,
		); err != nil {
			return nil, fmt.Errorf("failed to scan credential status: %w", err)
		}
		list = append(list, cs)
	}
	return list, rows.Err()
}

// UpdateCredentialStatus updates mutable fields of a credential status.
func (s *Store) UpdateCredentialStatus(cs *CredentialStatus) error {
	if cs == nil {
		return fmt.Errorf("credential status is nil")
	}
	_, err := s.db.Exec(`
		UPDATE qb_credential_statuses SET
			platform_id = ?, status = ?, detection_method = ?, detected_path = ?,
			checked_at = ?, message_redacted = ?, base_url = ?
		WHERE id = ?
	`,
		cs.PlatformID, cs.Status, cs.DetectionMethod, cs.DetectedPath,
		cs.CheckedAt, cs.MessageRedacted, cs.BaseURL,
		cs.ID,
	)
	if err != nil {
		return fmt.Errorf("failed to update credential status %d: %w", cs.ID, err)
	}
	return nil
}

// DeleteCredentialStatus deletes a credential status by id.
func (s *Store) DeleteCredentialStatus(id int64) error {
	_, err := s.db.Exec(`DELETE FROM qb_credential_statuses WHERE id = ?`, id)
	if err != nil {
		return fmt.Errorf("failed to delete credential status %d: %w", id, err)
	}
	return nil
}

// GetCredentialStatusByPlatform returns the credential status for a platform.
func (s *Store) GetCredentialStatusByPlatform(platformID int64) (*CredentialStatus, error) {
	var cs CredentialStatus
	err := s.db.QueryRow(`
		SELECT id, platform_id, status, detection_method, detected_path,
		       checked_at, message_redacted, base_url
		FROM qb_credential_statuses WHERE platform_id = ?
	`, platformID).Scan(
		&cs.ID, &cs.PlatformID, &cs.Status, &cs.DetectionMethod, &cs.DetectedPath,
		&cs.CheckedAt, &cs.MessageRedacted, &cs.BaseURL,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to get credential status by platform %d: %w", platformID, err)
	}
	return &cs, nil
}

// ---------------------------------------------------------------------------
// RiskNote CRUD + association + derived
// ---------------------------------------------------------------------------

// InsertRiskNote inserts a new risk note row and returns its ID.
func (s *Store) InsertRiskNote(rn *RiskNote) (int64, error) {
	if rn == nil {
		return 0, fmt.Errorf("risk note is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	res, err := s.db.Exec(`
		INSERT INTO qb_risk_notes (
			target_type, target_id, level, title, content, source,
			expires_at, resolved_at, created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		rn.TargetType, rn.TargetID, rn.Level, rn.Title, rn.Content, rn.Source,
		rn.ExpiresAt, rn.ResolvedAt, now, now,
	)
	if err != nil {
		return 0, fmt.Errorf("failed to insert risk note: %w", err)
	}
	id, err := res.LastInsertId()
	if err != nil {
		return 0, fmt.Errorf("failed to get risk note id: %w", err)
	}
	return id, nil
}

// GetRiskNote retrieves a risk note by id.
func (s *Store) GetRiskNote(id int64) (*RiskNote, error) {
	var rn RiskNote
	var targetID sql.NullInt64
	var createdAt, updatedAt string
	err := s.db.QueryRow(`
		SELECT id, target_type, target_id, level, title, content, source,
		       expires_at, resolved_at, created_at, updated_at
		FROM qb_risk_notes WHERE id = ?
	`, id).Scan(
		&rn.ID, &rn.TargetType, &targetID, &rn.Level, &rn.Title, &rn.Content, &rn.Source,
		&rn.ExpiresAt, &rn.ResolvedAt, &createdAt, &updatedAt,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to get risk note %d: %w", id, err)
	}
	if targetID.Valid {
		rn.TargetID = &targetID.Int64
	}
	rn.CreatedAt = mustParseTime(createdAt)
	rn.UpdatedAt = mustParseTime(updatedAt)
	return &rn, nil
}

// ListRiskNotes returns all risk notes.
func (s *Store) ListRiskNotes() ([]RiskNote, error) {
	rows, err := s.db.Query(`
		SELECT id, target_type, target_id, level, title, content, source,
		       expires_at, resolved_at, created_at, updated_at
		FROM qb_risk_notes ORDER BY created_at DESC
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to list risk notes: %w", err)
	}
	defer rows.Close()

	var list []RiskNote
	for rows.Next() {
		var rn RiskNote
		var targetID sql.NullInt64
		var createdAt, updatedAt string
		if err := rows.Scan(
			&rn.ID, &rn.TargetType, &targetID, &rn.Level, &rn.Title, &rn.Content, &rn.Source,
			&rn.ExpiresAt, &rn.ResolvedAt, &createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan risk note: %w", err)
		}
		if targetID.Valid {
			rn.TargetID = &targetID.Int64
		}
		rn.CreatedAt = mustParseTime(createdAt)
		rn.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, rn)
	}
	return list, rows.Err()
}

// UpdateRiskNote updates all mutable fields of a risk note.
func (s *Store) UpdateRiskNote(rn *RiskNote) error {
	if rn == nil {
		return fmt.Errorf("risk note is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	_, err := s.db.Exec(`
		UPDATE qb_risk_notes SET
			target_type = ?, target_id = ?, level = ?, title = ?, content = ?,
			source = ?, expires_at = ?, resolved_at = ?, updated_at = ?
		WHERE id = ?
	`,
		rn.TargetType, rn.TargetID, rn.Level, rn.Title, rn.Content,
		rn.Source, rn.ExpiresAt, rn.ResolvedAt, now,
		rn.ID,
	)
	if err != nil {
		return fmt.Errorf("failed to update risk note %d: %w", rn.ID, err)
	}
	return nil
}

// DeleteRiskNote deletes a risk note by id.
func (s *Store) DeleteRiskNote(id int64) error {
	_, err := s.db.Exec(`DELETE FROM qb_risk_notes WHERE id = ?`, id)
	if err != nil {
		return fmt.Errorf("failed to delete risk note %d: %w", id, err)
	}
	return nil
}

// ListRiskNotesByTarget returns risk notes for a given target type and id.
func (s *Store) ListRiskNotesByTarget(targetType string, targetID int64) ([]RiskNote, error) {
	rows, err := s.db.Query(`
		SELECT id, target_type, target_id, level, title, content, source,
		       expires_at, resolved_at, created_at, updated_at
		FROM qb_risk_notes
		WHERE target_type = ? AND target_id = ?
		ORDER BY created_at DESC
	`, targetType, targetID)
	if err != nil {
		return nil, fmt.Errorf("failed to list risk notes by target: %w", err)
	}
	defer rows.Close()

	var list []RiskNote
	for rows.Next() {
		var rn RiskNote
		var nullableTargetID sql.NullInt64
		var createdAt, updatedAt string
		if err := rows.Scan(
			&rn.ID, &rn.TargetType, &nullableTargetID, &rn.Level, &rn.Title, &rn.Content, &rn.Source,
			&rn.ExpiresAt, &rn.ResolvedAt, &createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan risk note: %w", err)
		}
		if nullableTargetID.Valid {
			rn.TargetID = &nullableTargetID.Int64
		}
		rn.CreatedAt = mustParseTime(createdAt)
		rn.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, rn)
	}
	return list, rows.Err()
}

// ListActiveRiskNotes returns risk notes that have not been resolved.
func (s *Store) ListActiveRiskNotes() ([]RiskNote, error) {
	rows, err := s.db.Query(`
		SELECT id, target_type, target_id, level, title, content, source,
		       expires_at, resolved_at, created_at, updated_at
		FROM qb_risk_notes
		WHERE resolved_at = ''
		ORDER BY created_at DESC
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to list active risk notes: %w", err)
	}
	defer rows.Close()

	var list []RiskNote
	for rows.Next() {
		var rn RiskNote
		var targetID sql.NullInt64
		var createdAt, updatedAt string
		if err := rows.Scan(
			&rn.ID, &rn.TargetType, &targetID, &rn.Level, &rn.Title, &rn.Content, &rn.Source,
			&rn.ExpiresAt, &rn.ResolvedAt, &createdAt, &updatedAt,
		); err != nil {
			return nil, fmt.Errorf("failed to scan risk note: %w", err)
		}
		if targetID.Valid {
			rn.TargetID = &targetID.Int64
		}
		rn.CreatedAt = mustParseTime(createdAt)
		rn.UpdatedAt = mustParseTime(updatedAt)
		list = append(list, rn)
	}
	return list, rows.Err()
}

// ---------------------------------------------------------------------------
// Import-specific batch methods (INSERT OR REPLACE with explicit ID)
// ---------------------------------------------------------------------------

// DeleteAllQB deletes all records from all qb_* tables in dependency-safe order
// (child tables first, parent tables last).
func (s *Store) DeleteAllQB() error {
	tables := []string{
		"qb_usage_logs",
		"qb_credential_statuses",
		"qb_risk_notes",
		"qb_models",
		"qb_quota_buckets",
		"qb_plans",
		"qb_platforms",
	}
	for _, tbl := range tables {
		if _, err := s.db.Exec("DELETE FROM " + tbl); err != nil {
			return fmt.Errorf("failed to delete %s: %w", tbl, err)
		}
	}
	return nil
}

// InsertPlatformWithID inserts or replaces a platform row with the given ID.
func (s *Store) InsertPlatformWithID(p *Platform) error {
	if p == nil {
		return fmt.Errorf("platform is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	_, err := s.db.Exec(`
		INSERT OR REPLACE INTO qb_platforms (
			id, name, vendor, category, base_url, credential_status,
			supports_tools_json, default_risk_level, is_active, notes,
			created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		p.ID, p.Name, p.Vendor, p.Category, p.BaseURL, p.CredentialStatus,
		boolToInt(p.SupportsToolsJSON), p.DefaultRiskLevel, boolToInt(p.IsActive), p.Notes,
		p.CreatedAt.UTC().Format(time.RFC3339Nano), now,
	)
	if err != nil {
		return fmt.Errorf("failed to insert/replace platform %d: %w", p.ID, err)
	}
	return nil
}

// InsertPlanWithID inserts or replaces a plan row with the given ID.
func (s *Store) InsertPlanWithID(pl *Plan) error {
	if pl == nil {
		return fmt.Errorf("plan is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	_, err := s.db.Exec(`
		INSERT OR REPLACE INTO qb_plans (
			id, platform_id, name, plan_type, starts_at, expires_at,
			renewal_policy, status, priority, recommended_role, risk_summary,
			created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		pl.ID, pl.PlatformID, pl.Name, pl.PlanType, pl.StartsAt, pl.ExpiresAt,
		pl.RenewalPolicy, pl.Status, pl.Priority, pl.RecommendedRole, pl.RiskSummary,
		pl.CreatedAt.UTC().Format(time.RFC3339Nano), now,
	)
	if err != nil {
		return fmt.Errorf("failed to insert/replace plan %d: %w", pl.ID, err)
	}
	return nil
}

// InsertQuotaBucketWithID inserts or replaces a quota bucket row with the given ID.
func (s *Store) InsertQuotaBucketWithID(qb *QuotaBucket) error {
	if qb == nil {
		return fmt.Errorf("quota bucket is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	_, err := s.db.Exec(`
		INSERT OR REPLACE INTO qb_quota_buckets (
			id, plan_id, scope, metric, limit_value, remaining_value, used_value,
			window_start, window_end, reset_at, source, confidence_level, notes,
			created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		qb.ID, qb.PlanID, qb.Scope, qb.Metric, qb.LimitValue, qb.RemainingValue, qb.UsedValue,
		qb.WindowStart, qb.WindowEnd, qb.ResetAt, qb.Source, qb.ConfidenceLevel, qb.Notes,
		qb.CreatedAt.UTC().Format(time.RFC3339Nano), now,
	)
	if err != nil {
		return fmt.Errorf("failed to insert/replace quota bucket %d: %w", qb.ID, err)
	}
	return nil
}

// InsertModelWithID inserts or replaces a model row with the given ID.
func (s *Store) InsertModelWithID(m *Model) error {
	if m == nil {
		return fmt.Errorf("model is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	_, err := s.db.Exec(`
		INSERT OR REPLACE INTO qb_models (
			id, platform_id, plan_id, model_id, display_name, family,
			is_current, base_url_override, tool_fit_json, status,
			created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		m.ID, m.PlatformID, m.PlanID, m.ModelID, m.DisplayName, m.Family,
		boolToInt(m.IsCurrent), m.BaseURLOverride, m.ToolFitJSON, m.Status,
		m.CreatedAt.UTC().Format(time.RFC3339Nano), now,
	)
	if err != nil {
		return fmt.Errorf("failed to insert/replace model %d: %w", m.ID, err)
	}
	return nil
}

// InsertUsageLogWithID inserts or replaces a usage log row with the given ID.
func (s *Store) InsertUsageLogWithID(ul *UsageLog) error {
	if ul == nil {
		return fmt.Errorf("usage log is nil")
	}
	_, err := s.db.Exec(`
		INSERT OR REPLACE INTO qb_usage_logs (
			id, plan_id, model_id, bucket_scope, date_key,
			period_start, period_end,
			input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
			request_count, cost_value, source, source_ref, created_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		ul.ID, ul.PlanID, ul.ModelID, ul.BucketScope, ul.DateKey,
		ul.PeriodStart, ul.PeriodEnd,
		ul.InputTokens, ul.OutputTokens, ul.CacheReadTokens, ul.CacheWriteTokens,
		ul.RequestCount, ul.CostValue, ul.Source, ul.SourceRef,
		ul.CreatedAt.UTC().Format(time.RFC3339Nano),
	)
	if err != nil {
		return fmt.Errorf("failed to insert/replace usage log %d: %w", ul.ID, err)
	}
	return nil
}

// InsertCredentialStatusWithID inserts or replaces a credential status row with the given ID.
func (s *Store) InsertCredentialStatusWithID(cs *CredentialStatus) error {
	if cs == nil {
		return fmt.Errorf("credential status is nil")
	}
	_, err := s.db.Exec(`
		INSERT OR REPLACE INTO qb_credential_statuses (
			id, platform_id, status, detection_method, detected_path,
			checked_at, message_redacted, base_url
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
	`,
		cs.ID, cs.PlatformID, cs.Status, cs.DetectionMethod, cs.DetectedPath,
		cs.CheckedAt, cs.MessageRedacted, cs.BaseURL,
	)
	if err != nil {
		return fmt.Errorf("failed to insert/replace credential status %d: %w", cs.ID, err)
	}
	return nil
}

// InsertRiskNoteWithID inserts or replaces a risk note row with the given ID.
func (s *Store) InsertRiskNoteWithID(rn *RiskNote) error {
	if rn == nil {
		return fmt.Errorf("risk note is nil")
	}
	now := time.Now().UTC().Format(time.RFC3339Nano)
	_, err := s.db.Exec(`
		INSERT OR REPLACE INTO qb_risk_notes (
			id, target_type, target_id, level, title, content, source,
			expires_at, resolved_at, created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		rn.ID, rn.TargetType, rn.TargetID, rn.Level, rn.Title, rn.Content, rn.Source,
		rn.ExpiresAt, rn.ResolvedAt,
		rn.CreatedAt.UTC().Format(time.RFC3339Nano), now,
	)
	if err != nil {
		return fmt.Errorf("failed to insert/replace risk note %d: %w", rn.ID, err)
	}
	return nil
}