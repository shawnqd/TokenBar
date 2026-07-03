package dashboard

import (
	"fmt"
	"time"

	"github.com/onllm-dev/onwatch/v2/internal/store"
)

// Recommendation provides a quick overview of what the user should pay
// attention to on the dashboard home page.
type Recommendation struct {
	TopPlan        *store.Plan          `json:"top_plan,omitempty"`
	TopModel       *store.Model         `json:"top_model,omitempty"`
	ExpiringPlans  []store.Plan         `json:"expiring_plans,omitempty"`
	LowQuotaBuckets []store.QuotaBucket `json:"low_quota_buckets,omitempty"`
	HighRiskNotes  []store.RiskNote     `json:"high_risk_notes,omitempty"`
	Reason         string               `json:"reason"`
}

// Recommend assembles a Recommendation from the current store data.
// Expiring plans are those ending within 7 days; low-quota buckets are those
// whose remaining/limit ratio is below 20%; high-risk notes are active notes
// with level "高" or "high". TopPlan is the active, unexpired plan with the
// highest priority. TopModel is the first model with is_current=true.
// All fields are safe to consume when nil/empty.
func Recommend(s *store.Store) (*Recommendation, error) {
	rec := &Recommendation{}

	now := time.Now().UTC()

	// ── Expiring plans (within 7 days) ────────────────────────────────────
	expiring, err := s.ListExpiringPlans(7)
	if err != nil {
		return nil, fmt.Errorf("recommend: list expiring plans: %w", err)
	}
	rec.ExpiringPlans = expiring

	// ── Low-quota buckets (remaining < 20% of limit) ──────────────────────
	lowBuckets, err := s.ListLowQuotaBuckets(0.2)
	if err != nil {
		return nil, fmt.Errorf("recommend: list low quota buckets: %w", err)
	}
	rec.LowQuotaBuckets = lowBuckets

	// ── Active risk notes, filtered to high-level ──────────────────────────
	activeNotes, err := s.ListActiveRiskNotes()
	if err != nil {
		return nil, fmt.Errorf("recommend: list active risk notes: %w", err)
	}
	for _, rn := range activeNotes {
		if rn.Level == "高" || rn.Level == "high" {
			rec.HighRiskNotes = append(rec.HighRiskNotes, rn)
		}
	}

	// ── Top plan: active, highest priority, not expired ────────────────────
	allPlans, err := s.ListPlans()
	if err != nil {
		return nil, fmt.Errorf("recommend: list plans: %w", err)
	}
	var bestPlan *store.Plan
	bestPriority := -1
	for _, pl := range allPlans {
		if pl.Status != "active" {
			continue
		}
		if pl.ExpiresAt != "" {
			expires, parseErr := time.Parse(time.RFC3339Nano, pl.ExpiresAt)
			if parseErr == nil && expires.Before(now) {
				continue
			}
		}
		if pl.Priority > bestPriority {
			p := pl // capture
			bestPlan = &p
			bestPriority = pl.Priority
		}
	}
	rec.TopPlan = bestPlan

	// ── Top model: first is_current=true ──────────────────────────────────
	allModels, err := s.ListModels()
	if err != nil {
		return nil, fmt.Errorf("recommend: list models: %w", err)
	}
	for _, m := range allModels {
		if m.IsCurrent {
			cp := m // capture
			rec.TopModel = &cp
			break
		}
	}

	// ── Build reason string ───────────────────────────────────────────────
	var parts []string
	if rec.TopPlan != nil {
		modelName := ""
		if rec.TopModel != nil {
			modelName = rec.TopModel.DisplayName
		}
		if modelName != "" {
			parts = append(parts, fmt.Sprintf("推荐优先使用 %s 的 %s", rec.TopPlan.Name, modelName))
		} else {
			parts = append(parts, fmt.Sprintf("推荐优先使用 %s", rec.TopPlan.Name))
		}
	} else if rec.TopModel != nil {
		parts = append(parts, fmt.Sprintf("当前模型 %s 可用", rec.TopModel.DisplayName))
	}

	if len(rec.ExpiringPlans) > 0 {
		parts = append(parts, fmt.Sprintf("注意 %d 个套餐即将到期", len(rec.ExpiringPlans)))
	}
	if len(rec.LowQuotaBuckets) > 0 {
		parts = append(parts, fmt.Sprintf("%d 个额度桶不足", len(rec.LowQuotaBuckets)))
	}
	if len(rec.HighRiskNotes) > 0 {
		parts = append(parts, fmt.Sprintf("%d 条高风险备注待处理", len(rec.HighRiskNotes)))
	}

	if len(parts) == 0 {
		parts = append(parts, "暂无推荐，请先添加平台数据")
	}
	rec.Reason = parts[0]
	for _, p := range parts[1:] {
		rec.Reason += "；" + p
	}

	return rec, nil
}