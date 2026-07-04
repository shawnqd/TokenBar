package dashboard

import (
	"fmt"
	"time"

	"github.com/onllm-dev/onwatch/v2/internal/store"
)

// SeedSampleData inserts a set of sample platforms, plans, quota buckets,
// models, credential statuses, and risk notes if no platforms exist yet.
// It is safe to call multiple times — the first call seeds, subsequent calls
// return nil immediately.
func SeedSampleData(s *store.Store) error {
	existing, err := s.ListPlatforms()
	if err != nil {
		return fmt.Errorf("seed: list platforms: %w", err)
	}
	if len(existing) > 0 {
		return nil // already seeded
	}

	now := time.Now().UTC()
	fmtNow := now.Format(time.RFC3339Nano)

	// Helper: future date string N days from now.
	future := func(days int) string {
		return now.AddDate(0, 0, days).Format(time.RFC3339Nano)
	}
	past := func(days int) string {
		return now.AddDate(0, 0, -days).Format(time.RFC3339Nano)
	}

	// ── Platforms ──────────────────────────────────────────────────────────

	huoshanID, err := s.InsertPlatform(&store.Platform{
		Name:              "火山方舟",
		Vendor:            "火山引擎",
		Category:          "api",
		BaseURL:           "https://ark.cn-beijing.volces.com/api/v3",
		CredentialStatus:  "已配置",
		DefaultRiskLevel:  "low",
		SupportsToolsJSON: true,
		IsActive:          true,
		Notes:             "",
	})
	if err != nil {
		return fmt.Errorf("seed: insert 火山方舟: %w", err)
	}

	mimoID, err := s.InsertPlatform(&store.Platform{
		Name:             "MiMo",
		Vendor:           "MiMo",
		Category:         "api",
		BaseURL:          "https://api.mimo.cn/v1",
		CredentialStatus: "已配置",
		DefaultRiskLevel: "low",
		IsActive:         true,
	})
	if err != nil {
		return fmt.Errorf("seed: insert MiMo: %w", err)
	}

	senseID, err := s.InsertPlatform(&store.Platform{
		Name:             "SenseNova",
		Vendor:           "商汤",
		Category:         "token_plan",
		BaseURL:          "https://api.sensetime.com/v1",
		CredentialStatus: "已配置",
		DefaultRiskLevel: "low",
		IsActive:         true,
	})
	if err != nil {
		return fmt.Errorf("seed: insert SenseNova: %w", err)
	}

	opencodeID, err := s.InsertPlatform(&store.Platform{
		Name:             "OpenCode Zen",
		Vendor:           "OpenCode",
		Category:         "free",
		BaseURL:          "https://api.opencode.ai",
		CredentialStatus: "待验证",
		DefaultRiskLevel: "medium",
		IsActive:         true,
	})
	if err != nil {
		return fmt.Errorf("seed: insert OpenCode Zen: %w", err)
	}

	// ── Plans ──────────────────────────────────────────────────────────────

	huoshanPlanID, err := s.InsertPlan(&store.Plan{
		PlatformID:    huoshanID,
		Name:          "Coding Plan",
		PlanType:      "subscription",
		StartsAt:      past(30),
		ExpiresAt:     future(90),
		RenewalPolicy: "auto",
		Status:        "active",
		Priority:      10,
		RiskSummary:   "",
	})
	if err != nil {
		return fmt.Errorf("seed: insert 火山方舟 plan: %w", err)
	}

	mimoPlanID, err := s.InsertPlan(&store.Plan{
		PlatformID:    mimoID,
		Name:          "Token Plan",
		PlanType:      "token",
		StartsAt:      past(30),
		ExpiresAt:     future(5), // expires soon – for expiring-plans testing
		RenewalPolicy: "manual",
		Status:        "active",
		Priority:      8,
		RiskSummary:   "",
	})
	if err != nil {
		return fmt.Errorf("seed: insert MiMo plan: %w", err)
	}

	sensePlanID, err := s.InsertPlan(&store.Plan{
		PlatformID:    senseID,
		Name:          "Token Plan Free",
		PlanType:      "free",
		StartsAt:      past(30),
		ExpiresAt:     future(60),
		RenewalPolicy: "none",
		Status:        "active",
		Priority:      5,
		RiskSummary:   "",
	})
	if err != nil {
		return fmt.Errorf("seed: insert SenseNova plan: %w", err)
	}

	opencodePlanID, err := s.InsertPlan(&store.Plan{
		PlatformID:    opencodeID,
		Name:          "Free",
		PlanType:      "free",
		StartsAt:      past(30),
		ExpiresAt:     future(30),
		RenewalPolicy: "none",
		Status:        "active",
		Priority:      3,
		RiskSummary:   "",
	})
	if err != nil {
		return fmt.Errorf("seed: insert OpenCode Zen plan: %w", err)
	}

	// ── Quota Buckets ──────────────────────────────────────────────────────

	if _, err := s.InsertQuotaBucket(&store.QuotaBucket{
		PlanID:          huoshanPlanID,
		Scope:           "month",
		Metric:          "tokens",
		LimitValue:      1_000_000,
		RemainingValue:  800_000,
		UsedValue:       200_000,
		WindowStart:     fmtNow,
		WindowEnd:       future(30),
		ResetAt:         future(30),
		Source:          "manual",
		ConfidenceLevel: "high",
	}); err != nil {
		return fmt.Errorf("seed: insert 火山方舟 quota bucket: %w", err)
	}

	if _, err := s.InsertQuotaBucket(&store.QuotaBucket{
		PlanID:          mimoPlanID,
		Scope:           "month",
		Metric:          "tokens",
		LimitValue:      500_000,
		RemainingValue:  50_000, // low (<20%) – triggers low-quota alert
		UsedValue:       450_000,
		WindowStart:     fmtNow,
		WindowEnd:       future(30),
		ResetAt:         future(30),
		Source:          "manual",
		ConfidenceLevel: "medium",
	}); err != nil {
		return fmt.Errorf("seed: insert MiMo quota bucket: %w", err)
	}

	if _, err := s.InsertQuotaBucket(&store.QuotaBucket{
		PlanID:          sensePlanID,
		Scope:           "month",
		Metric:          "tokens",
		LimitValue:      200_000,
		RemainingValue:  30_000, // low (<20%)
		UsedValue:       170_000,
		WindowStart:     fmtNow,
		WindowEnd:       future(30),
		ResetAt:         future(30),
		Source:          "manual",
		ConfidenceLevel: "low",
	}); err != nil {
		return fmt.Errorf("seed: insert SenseNova quota bucket: %w", err)
	}

	if _, err := s.InsertQuotaBucket(&store.QuotaBucket{
		PlanID:          opencodePlanID,
		Scope:           "month",
		Metric:          "tokens",
		LimitValue:      10_000,
		RemainingValue:  5_000, // 50% – fine
		UsedValue:       5_000,
		WindowStart:     fmtNow,
		WindowEnd:       future(30),
		ResetAt:         future(30),
		Source:          "manual",
		ConfidenceLevel: "medium",
	}); err != nil {
		return fmt.Errorf("seed: insert OpenCode Zen quota bucket: %w", err)
	}

	// ── Models ─────────────────────────────────────────────────────────────

	doubaoPlanID := huoshanPlanID
	if _, err := s.InsertModel(&store.Model{
		PlatformID:      huoshanID,
		PlanID:          &doubaoPlanID,
		ModelID:         "doubao-coding",
		DisplayName:     "豆包 Coding",
		Family:          "doubao",
		IsCurrent:       true,
		BaseURLOverride: "",
		ToolFitJSON:     `{"tools":true,"json_mode":true}`,
		Status:          "active",
	}); err != nil {
		return fmt.Errorf("seed: insert doubao-coding: %w", err)
	}

	mimoModelPlanID := mimoPlanID
	if _, err := s.InsertModel(&store.Model{
		PlatformID:      mimoID,
		PlanID:          &mimoModelPlanID,
		ModelID:         "mimo-code",
		DisplayName:     "MiMo Code",
		Family:          "mimo",
		IsCurrent:       true,
		BaseURLOverride: "",
		Status:          "active",
	}); err != nil {
		return fmt.Errorf("seed: insert mimo-code: %w", err)
	}

	senseModelPlanID := sensePlanID
	if _, err := s.InsertModel(&store.Model{
		PlatformID:      senseID,
		PlanID:          &senseModelPlanID,
		ModelID:         "sensecode",
		DisplayName:     "SenseCode",
		Family:          "sense",
		IsCurrent:       true,
		BaseURLOverride: "",
		Status:          "active",
	}); err != nil {
		return fmt.Errorf("seed: insert sensecode: %w", err)
	}

	zenModelPlanID := opencodePlanID
	if _, err := s.InsertModel(&store.Model{
		PlatformID:      opencodeID,
		PlanID:          &zenModelPlanID,
		ModelID:         "zen-model",
		DisplayName:     "Zen Model",
		Family:          "zen",
		IsCurrent:       true,
		BaseURLOverride: "",
		Status:          "active",
	}); err != nil {
		return fmt.Errorf("seed: insert zen-model: %w", err)
	}

	// ── Credential Statuses ────────────────────────────────────────────────

	if _, err := s.InsertCredentialStatus(&store.CredentialStatus{
		PlatformID:      huoshanID,
		Status:          "已配置",
		DetectionMethod: "manual",
		DetectedPath:    "",
		CheckedAt:       fmtNow,
		MessageRedacted: "",
		BaseURL:         "https://ark.cn-beijing.volces.com/api/v3",
	}); err != nil {
		return fmt.Errorf("seed: insert 火山方舟 credential status: %w", err)
	}

	if _, err := s.InsertCredentialStatus(&store.CredentialStatus{
		PlatformID:      mimoID,
		Status:          "已配置",
		DetectionMethod: "manual",
		DetectedPath:    "",
		CheckedAt:       fmtNow,
		MessageRedacted: "",
		BaseURL:         "https://api.mimo.cn/v1",
	}); err != nil {
		return fmt.Errorf("seed: insert MiMo credential status: %w", err)
	}

	if _, err := s.InsertCredentialStatus(&store.CredentialStatus{
		PlatformID:      senseID,
		Status:          "已配置",
		DetectionMethod: "manual",
		DetectedPath:    "",
		CheckedAt:       fmtNow,
		MessageRedacted: "",
		BaseURL:         "https://api.sensetime.com/v1",
	}); err != nil {
		return fmt.Errorf("seed: insert SenseNova credential status: %w", err)
	}

	if _, err := s.InsertCredentialStatus(&store.CredentialStatus{
		PlatformID:      opencodeID,
		Status:          "待验证",
		DetectionMethod: "manual",
		DetectedPath:    "",
		CheckedAt:       fmtNow,
		MessageRedacted: "",
		BaseURL:         "https://api.opencode.ai",
	}); err != nil {
		return fmt.Errorf("seed: insert OpenCode Zen credential status: %w", err)
	}

	// ── Risk Notes ─────────────────────────────────────────────────────────

	huoshanTargetID := huoshanID
	if _, err := s.InsertRiskNote(&store.RiskNote{
		TargetType: "platform",
		TargetID:   &huoshanTargetID,
		Level:      "medium",
		Title:      "额度重置窗口注意",
		Content:    "火山方舟的包年额度将在重置窗口后刷新，请注意持续监控。",
		Source:     "manual",
	}); err != nil {
		return fmt.Errorf("seed: insert 火山方舟 risk note: %w", err)
	}

	senseTargetID := senseID
	if _, err := s.InsertRiskNote(&store.RiskNote{
		TargetType: "platform",
		TargetID:   &senseTargetID,
		Level:      "low",
		Title:      "免费额度有限",
		Content:    "SenseNova 免费额度仅余 15%，建议关注使用节奏。",
		Source:     "manual",
	}); err != nil {
		return fmt.Errorf("seed: insert SenseNova risk note: %w", err)
	}

	// ── Usage Logs (sample consumption across periods) ────────────────────
	// today: should appear in today / week / month summaries
	huoshanUsagePlanID := huoshanPlanID
	if _, err := s.InsertUsageLog(&store.UsageLog{
		PlanID:       &huoshanUsagePlanID,
		BucketScope:  "day",
		DateKey:      now.Format("2006-01-02"),
		PeriodStart:  now.Format(time.RFC3339Nano),
		PeriodEnd:    now.Format(time.RFC3339Nano),
		InputTokens:  12000,
		OutputTokens: 8000,
		RequestCount: 15,
		CostValue:    0.12,
		Source:       "manual",
	}); err != nil {
		return fmt.Errorf("seed: insert huoshan usage log: %w", err)
	}
	// earlier this month (10 days ago): in month, maybe not in today/week
	mimoUsagePlanID := mimoPlanID
	tenDaysAgo := now.AddDate(0, 0, -10)
	if _, err := s.InsertUsageLog(&store.UsageLog{
		PlanID:       &mimoUsagePlanID,
		BucketScope:  "day",
		DateKey:      tenDaysAgo.Format("2006-01-02"),
		PeriodStart:  tenDaysAgo.Format(time.RFC3339Nano),
		PeriodEnd:    tenDaysAgo.Format(time.RFC3339Nano),
		InputTokens:  50000,
		OutputTokens: 20000,
		RequestCount: 40,
		CostValue:    0.50,
		Source:       "manual",
	}); err != nil {
		return fmt.Errorf("seed: insert mimo usage log: %w", err)
	}
	// last month (40 days ago): should NOT appear in this month summary
	senseUsagePlanID := sensePlanID
	fortyDaysAgo := now.AddDate(0, 0, -40)
	if _, err := s.InsertUsageLog(&store.UsageLog{
		PlanID:       &senseUsagePlanID,
		BucketScope:  "day",
		DateKey:      fortyDaysAgo.Format("2006-01-02"),
		PeriodStart:  fortyDaysAgo.Format(time.RFC3339Nano),
		PeriodEnd:    fortyDaysAgo.Format(time.RFC3339Nano),
		InputTokens:  999,
		OutputTokens: 999,
		RequestCount: 1,
		CostValue:    0.01,
		Source:       "manual",
	}); err != nil {
		return fmt.Errorf("seed: insert sense usage log: %w", err)
	}

	return nil
}
