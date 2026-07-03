package web

import (
	"io"
	"net/http"
	"strconv"
	"time"

	"github.com/onllm-dev/onwatch/v2/internal/dashboard"
	"github.com/onllm-dev/onwatch/v2/internal/store"
)

// ---------------------------------------------------------------------------
// Page handlers (GET, render template)
// ---------------------------------------------------------------------------

// OverviewPage renders the quota board overview (recommendation + platforms).
func (h *Handler) OverviewPage(w http.ResponseWriter, r *http.Request) {
	rec, err := dashboard.Recommend(h.store)
	if err != nil {
		h.logger.Error("dashboard.Recommend failed", "error", err)
		rec = nil
	}

	platforms, err := h.store.ListPlatforms()
	if err != nil {
		h.logger.Error("ListPlatforms failed", "error", err)
		platforms = nil
	}

	data := map[string]interface{}{
		"Title":     "模型额度看板",
		"BasePath":  h.getBasePath(),
		"Version":   h.version,
		"Nav":       "overview",
		"Rec":       rec,
		"Platforms": platforms,
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.overviewTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render overview template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// PlansPage renders the plan management page with bucket info per plan.
func (h *Handler) PlansPage(w http.ResponseWriter, r *http.Request) {
	platforms, err := h.store.ListPlatforms()
	if err != nil {
		h.logger.Error("ListPlatforms failed", "error", err)
		platforms = nil
	}

	plans, err := h.store.ListPlans()
	if err != nil {
		h.logger.Error("ListPlans failed", "error", err)
		plans = nil
	}

	bucketsByPlan := make(map[int64][]store.QuotaBucket)
	for _, plan := range plans {
		buckets, err := h.store.ListQuotaBucketsByPlan(plan.ID)
		if err != nil {
			h.logger.Error("ListQuotaBucketsByPlan failed", "plan_id", plan.ID, "error", err)
			buckets = nil
		}
		bucketsByPlan[plan.ID] = buckets
	}

	modelsByPlatform := make(map[int64][]store.Model)
	for _, platform := range platforms {
		models, err := h.store.ListModelsByPlatform(platform.ID)
		if err != nil {
			h.logger.Error("ListModelsByPlatform failed", "platform_id", platform.ID, "error", err)
			models = nil
		}
		modelsByPlatform[platform.ID] = models
	}

	data := map[string]interface{}{
		"Title":            "套餐管理",
		"BasePath":         h.getBasePath(),
		"Version":          h.version,
		"Nav":              "plans",
		"Platforms":        platforms,
		"Plans":            plans,
		"BucketsByPlan":    bucketsByPlan,
		"ModelsByPlatform": modelsByPlatform,
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.plansTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render plans template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// ProvidersPage renders provider credential status page.
func (h *Handler) ProvidersPage(w http.ResponseWriter, r *http.Request) {
	credStatuses, err := h.store.ListCredentialStatuses()
	if err != nil {
		h.logger.Error("ListCredentialStatuses failed", "error", err)
		credStatuses = nil
	}

	platforms, err := h.store.ListPlatforms()
	if err != nil {
		h.logger.Error("ListPlatforms failed", "error", err)
		platforms = nil
	}

	data := map[string]interface{}{
		"Title":              "Provider 状态",
		"BasePath":           h.getBasePath(),
		"Version":            h.version,
		"Nav":                "providers",
		"CredentialStatuses": credStatuses,
		"Platforms":          platforms,
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.providersTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render providers template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// UsagePage renders usage log page with period summaries per plan.
func (h *Handler) UsagePage(w http.ResponseWriter, r *http.Request) {
	now := time.Now()
	periodStart := now.AddDate(0, 0, -30).Format(time.RFC3339)
	periodEnd := now.Format(time.RFC3339)

	usageLogs, err := h.store.ListUsageLogs()
	if err != nil {
		h.logger.Error("ListUsageLogs failed", "error", err)
		usageLogs = nil
	}

	platforms, err := h.store.ListPlatforms()
	if err != nil {
		h.logger.Error("ListPlatforms failed", "error", err)
		platforms = nil
	}

	plans, err := h.store.ListPlans()
	if err != nil {
		h.logger.Error("ListPlans failed", "error", err)
		plans = nil
	}

	summaries := make(map[int64]store.UsageSummary)
	for _, plan := range plans {
		summary, err := h.store.UsageSummaryByPeriod(plan.ID, periodStart, periodEnd)
		if err != nil {
			h.logger.Error("UsageSummaryByPeriod failed", "plan_id", plan.ID, "error", err)
			continue
		}
		summaries[plan.ID] = summary
	}

	data := map[string]interface{}{
		"Title":       "使用记录",
		"BasePath":    h.getBasePath(),
		"Version":     h.version,
		"Nav":         "usage",
		"UsageLogs":   usageLogs,
		"Platforms":   platforms,
		"Summaries":   summaries,
		"PeriodStart": periodStart,
		"PeriodEnd":   periodEnd,
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.usageTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render usage template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// ConfigPage renders configuration status page with platforms and credential statuses.
func (h *Handler) ConfigPage(w http.ResponseWriter, r *http.Request) {
	platforms, err := h.store.ListPlatforms()
	if err != nil {
		h.logger.Error("ListPlatforms failed", "error", err)
		platforms = nil
	}

	credStatuses, err := h.store.ListCredentialStatuses()
	if err != nil {
		h.logger.Error("ListCredentialStatuses failed", "error", err)
		credStatuses = nil
	}

	data := map[string]interface{}{
		"Title":              "配置状态",
		"BasePath":           h.getBasePath(),
		"Version":            h.version,
		"Nav":                "config",
		"Platforms":          platforms,
		"CredentialStatuses": credStatuses,
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.configTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render config template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// RisksPage renders risk notes page.
func (h *Handler) RisksPage(w http.ResponseWriter, r *http.Request) {
	riskNotes, err := h.store.ListRiskNotes()
	if err != nil {
		h.logger.Error("ListRiskNotes failed", "error", err)
		riskNotes = nil
	}

	data := map[string]interface{}{
		"Title":     "风险备注",
		"BasePath":  h.getBasePath(),
		"Version":   h.version,
		"Nav":       "risks",
		"RiskNotes": riskNotes,
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.risksTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render risks template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// ImportExportPage renders the import/export page.
func (h *Handler) ImportExportPage(w http.ResponseWriter, r *http.Request) {
	data := map[string]interface{}{
		"Title":        "导入导出",
		"BasePath":     h.getBasePath(),
		"Version":      h.version,
		"Nav":          "import-export",
		"ImportResult": nil,
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.importExportTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render import-export template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// ---------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------

// SeedAction seeds sample data via dashboard.SeedSampleData.
func (h *Handler) SeedAction(w http.ResponseWriter, r *http.Request) {
	if err := dashboard.SeedSampleData(h.store); err != nil {
		h.logger.Error("SeedSampleData failed", "error", err)
		http.Error(w, "Failed to seed sample data: "+err.Error(), http.StatusInternalServerError)
		return
	}
	http.Redirect(w, r, h.getBasePath()+"/qb/", http.StatusFound)
}

// ExportAction exports quota board data as JSON download.
func (h *Handler) ExportAction(w http.ResponseWriter, r *http.Request) {
	data, err := dashboard.ExportData(h.store)
	if err != nil {
		h.logger.Error("ExportData failed", "error", err)
		http.Error(w, "Failed to export data: "+err.Error(), http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Content-Disposition", `attachment; filename="quota-board-export.json"`)
	w.Write(data)
}

// ImportAction handles import of quota board data from a JSON file upload.
// Requires X-Requested-With header (enforced by csrfMiddleware for POST).
// Returns JSON for fetch-based form submission.
func (h *Handler) ImportAction(w http.ResponseWriter, r *http.Request) {
	if err := r.ParseMultipartForm(10 << 20); err != nil {
		h.logger.Error("ParseMultipartForm failed", "error", err)
		respondError(w, http.StatusBadRequest, "Failed to parse multipart form: "+err.Error())
		return
	}

	file, _, err := r.FormFile("file")
	if err != nil {
		h.logger.Error("FormFile failed", "error", err)
		respondError(w, http.StatusBadRequest, "Missing file field: "+err.Error())
		return
	}
	defer file.Close()

	body, err := io.ReadAll(file)
	if err != nil {
		h.logger.Error("reading uploaded file failed", "error", err)
		respondError(w, http.StatusInternalServerError, "Failed to read file: "+err.Error())
		return
	}

	result, err := dashboard.ImportData(h.store, body, "upsert")
	if err != nil {
		h.logger.Error("ImportData failed", "error", err)
		respondError(w, http.StatusBadRequest, err.Error())
		return
	}

	respondJSON(w, http.StatusOK, map[string]interface{}{
		"redirect": h.getBasePath() + "/qb/import-export",
		"result":   result,
	})
}

// ---------------------------------------------------------------------------
// Platform form handlers
// ---------------------------------------------------------------------------

// PlatformNewForm renders the new platform form.
func (h *Handler) PlatformNewForm(w http.ResponseWriter, r *http.Request) {
	data := map[string]interface{}{
		"Title":    "新增平台",
		"BasePath": h.getBasePath(),
		"Version":  h.version,
		"Nav":      "plans",
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.platformFormTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render platform form template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// PlatformEditForm renders the edit platform form.
func (h *Handler) PlatformEditForm(w http.ResponseWriter, r *http.Request) {
	idStr := r.URL.Query().Get("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil || id <= 0 {
		http.Error(w, "invalid platform id", http.StatusBadRequest)
		return
	}
	platform, err := h.store.GetPlatform(id)
	if err != nil {
		h.logger.Error("GetPlatform failed", "error", err)
		http.Error(w, "platform not found", http.StatusNotFound)
		return
	}
	data := map[string]interface{}{
		"Title":    "编辑平台",
		"BasePath": h.getBasePath(),
		"Version":  h.version,
		"Nav":      "plans",
		"Platform": platform,
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.platformFormTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render platform form template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// PlatformSave handles platform create/update.
func (h *Handler) PlatformSave(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		respondError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}
	if err := r.ParseForm(); err != nil {
		respondError(w, http.StatusBadRequest, "failed to parse form")
		return
	}

	p := &store.Platform{
		Name:             r.FormValue("name"),
		Vendor:           r.FormValue("vendor"),
		Category:         r.FormValue("category"),
		BaseURL:          r.FormValue("base_url"),
		CredentialStatus: r.FormValue("credential_status"),
		DefaultRiskLevel: r.FormValue("default_risk_level"),
		IsActive:         r.FormValue("is_active") == "1",
		Notes:            r.FormValue("notes"),
	}

	idStr := r.FormValue("id")
	if idStr != "" {
		id, err := strconv.ParseInt(idStr, 10, 64)
		if err != nil {
			respondError(w, http.StatusBadRequest, "invalid id")
			return
		}
		existing, err := h.store.GetPlatform(id)
		if err != nil {
			h.logger.Error("GetPlatform failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to get platform")
			return
		}
		p.ID = existing.ID
		p.CreatedAt = existing.CreatedAt
		if err := h.store.UpdatePlatform(p); err != nil {
			h.logger.Error("UpdatePlatform failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to update platform")
			return
		}
	} else {
		if _, err := h.store.InsertPlatform(p); err != nil {
			h.logger.Error("InsertPlatform failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to create platform")
			return
		}
	}

	respondJSON(w, http.StatusOK, map[string]string{"redirect": h.getBasePath() + "/qb/plans"})
}

// PlatformDelete handles platform deletion.
func (h *Handler) PlatformDelete(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		respondError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}
	idStr := r.URL.Query().Get("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil || id <= 0 {
		respondError(w, http.StatusBadRequest, "invalid platform id")
		return
	}
	if err := h.store.DeletePlatform(id); err != nil {
		h.logger.Error("DeletePlatform failed", "error", err)
		respondError(w, http.StatusInternalServerError, "failed to delete platform")
		return
	}
	respondJSON(w, http.StatusOK, map[string]string{"redirect": h.getBasePath() + "/qb/plans"})
}

// ---------------------------------------------------------------------------
// Plan form handlers
// ---------------------------------------------------------------------------

// PlanNewForm renders the new plan form.
func (h *Handler) PlanNewForm(w http.ResponseWriter, r *http.Request) {
	platformIDStr := r.URL.Query().Get("platform_id")
	platformID, err := strconv.ParseInt(platformIDStr, 10, 64)
	if err != nil || platformID <= 0 {
		http.Error(w, "invalid platform_id", http.StatusBadRequest)
		return
	}
	data := map[string]interface{}{
		"Title":      "新增套餐",
		"BasePath":   h.getBasePath(),
		"Version":    h.version,
		"Nav":        "plans",
		"PlatformID": platformID,
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.planFormTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render plan form template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// PlanEditForm renders the edit plan form.
func (h *Handler) PlanEditForm(w http.ResponseWriter, r *http.Request) {
	idStr := r.URL.Query().Get("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil || id <= 0 {
		http.Error(w, "invalid plan id", http.StatusBadRequest)
		return
	}
	plan, err := h.store.GetPlan(id)
	if err != nil {
		h.logger.Error("GetPlan failed", "error", err)
		http.Error(w, "plan not found", http.StatusNotFound)
		return
	}
	data := map[string]interface{}{
		"Title":      "编辑套餐",
		"BasePath":   h.getBasePath(),
		"Version":    h.version,
		"Nav":        "plans",
		"Plan":       plan,
		"PlatformID": plan.PlatformID,
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.planFormTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render plan form template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// PlanSave handles plan create/update.
func (h *Handler) PlanSave(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		respondError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}
	if err := r.ParseForm(); err != nil {
		respondError(w, http.StatusBadRequest, "failed to parse form")
		return
	}

	priority, _ := strconv.Atoi(r.FormValue("priority"))
	platformID, _ := strconv.ParseInt(r.FormValue("platform_id"), 10, 64)

	pl := &store.Plan{
		PlatformID:      platformID,
		Name:            r.FormValue("name"),
		PlanType:        r.FormValue("plan_type"),
		StartsAt:        r.FormValue("starts_at"),
		ExpiresAt:       r.FormValue("expires_at"),
		RenewalPolicy:   r.FormValue("renewal_policy"),
		Status:          r.FormValue("status"),
		Priority:        priority,
		RecommendedRole: r.FormValue("recommended_role"),
		RiskSummary:     r.FormValue("risk_summary"),
	}

	idStr := r.FormValue("id")
	if idStr != "" {
		id, err := strconv.ParseInt(idStr, 10, 64)
		if err != nil {
			respondError(w, http.StatusBadRequest, "invalid id")
			return
		}
		existing, err := h.store.GetPlan(id)
		if err != nil {
			h.logger.Error("GetPlan failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to get plan")
			return
		}
		pl.ID = existing.ID
		pl.CreatedAt = existing.CreatedAt
		if err := h.store.UpdatePlan(pl); err != nil {
			h.logger.Error("UpdatePlan failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to update plan")
			return
		}
	} else {
		if _, err := h.store.InsertPlan(pl); err != nil {
			h.logger.Error("InsertPlan failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to create plan")
			return
		}
	}

	respondJSON(w, http.StatusOK, map[string]string{"redirect": h.getBasePath() + "/qb/plans"})
}

// PlanDelete handles plan deletion.
func (h *Handler) PlanDelete(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		respondError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}
	idStr := r.URL.Query().Get("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil || id <= 0 {
		respondError(w, http.StatusBadRequest, "invalid plan id")
		return
	}
	if err := h.store.DeletePlan(id); err != nil {
		h.logger.Error("DeletePlan failed", "error", err)
		respondError(w, http.StatusInternalServerError, "failed to delete plan")
		return
	}
	respondJSON(w, http.StatusOK, map[string]string{"redirect": h.getBasePath() + "/qb/plans"})
}

// ---------------------------------------------------------------------------
// QuotaBucket form handlers
// ---------------------------------------------------------------------------

// BucketNewForm renders the new quota bucket form.
func (h *Handler) BucketNewForm(w http.ResponseWriter, r *http.Request) {
	planIDStr := r.URL.Query().Get("plan_id")
	planID, err := strconv.ParseInt(planIDStr, 10, 64)
	if err != nil || planID <= 0 {
		http.Error(w, "invalid plan_id", http.StatusBadRequest)
		return
	}
	data := map[string]interface{}{
		"Title":    "新增额度桶",
		"BasePath": h.getBasePath(),
		"Version":  h.version,
		"Nav":      "plans",
		"PlanID":   planID,
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.bucketFormTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render bucket form template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// BucketEditForm renders the edit quota bucket form.
func (h *Handler) BucketEditForm(w http.ResponseWriter, r *http.Request) {
	idStr := r.URL.Query().Get("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil || id <= 0 {
		http.Error(w, "invalid bucket id", http.StatusBadRequest)
		return
	}
	bucket, err := h.store.GetQuotaBucket(id)
	if err != nil {
		h.logger.Error("GetQuotaBucket failed", "error", err)
		http.Error(w, "bucket not found", http.StatusNotFound)
		return
	}
	data := map[string]interface{}{
		"Title":    "编辑额度桶",
		"BasePath": h.getBasePath(),
		"Version":  h.version,
		"Nav":      "plans",
		"Bucket":   bucket,
		"PlanID":   bucket.PlanID,
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.bucketFormTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render bucket form template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// BucketSave handles quota bucket create/update.
func (h *Handler) BucketSave(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		respondError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}
	if err := r.ParseForm(); err != nil {
		respondError(w, http.StatusBadRequest, "failed to parse form")
		return
	}

	planID, _ := strconv.ParseInt(r.FormValue("plan_id"), 10, 64)
	limitValue, _ := strconv.ParseFloat(r.FormValue("limit_value"), 64)
	remainingValue, _ := strconv.ParseFloat(r.FormValue("remaining_value"), 64)
	usedValue, _ := strconv.ParseFloat(r.FormValue("used_value"), 64)

	qb := &store.QuotaBucket{
		PlanID:          planID,
		Scope:           r.FormValue("scope"),
		Metric:          r.FormValue("metric"),
		LimitValue:      limitValue,
		RemainingValue:  remainingValue,
		UsedValue:       usedValue,
		WindowStart:     r.FormValue("window_start"),
		WindowEnd:       r.FormValue("window_end"),
		ResetAt:         r.FormValue("reset_at"),
		Source:          r.FormValue("source"),
		ConfidenceLevel: r.FormValue("confidence_level"),
		Notes:           r.FormValue("notes"),
	}

	idStr := r.FormValue("id")
	if idStr != "" {
		id, err := strconv.ParseInt(idStr, 10, 64)
		if err != nil {
			respondError(w, http.StatusBadRequest, "invalid id")
			return
		}
		existing, err := h.store.GetQuotaBucket(id)
		if err != nil {
			h.logger.Error("GetQuotaBucket failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to get bucket")
			return
		}
		qb.ID = existing.ID
		qb.CreatedAt = existing.CreatedAt
		if err := h.store.UpdateQuotaBucket(qb); err != nil {
			h.logger.Error("UpdateQuotaBucket failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to update bucket")
			return
		}
	} else {
		if _, err := h.store.InsertQuotaBucket(qb); err != nil {
			h.logger.Error("InsertQuotaBucket failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to create bucket")
			return
		}
	}

	respondJSON(w, http.StatusOK, map[string]string{"redirect": h.getBasePath() + "/qb/plans"})
}

// BucketDelete handles quota bucket deletion.
func (h *Handler) BucketDelete(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		respondError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}
	idStr := r.URL.Query().Get("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil || id <= 0 {
		respondError(w, http.StatusBadRequest, "invalid bucket id")
		return
	}
	if err := h.store.DeleteQuotaBucket(id); err != nil {
		h.logger.Error("DeleteQuotaBucket failed", "error", err)
		respondError(w, http.StatusInternalServerError, "failed to delete bucket")
		return
	}
	respondJSON(w, http.StatusOK, map[string]string{"redirect": h.getBasePath() + "/qb/plans"})
}

// ---------------------------------------------------------------------------
// RiskNote form handlers
// ---------------------------------------------------------------------------

// RiskNewForm renders the new risk note form.
func (h *Handler) RiskNewForm(w http.ResponseWriter, r *http.Request) {
	targetType := r.URL.Query().Get("target_type")
	targetIDStr := r.URL.Query().Get("target_id")
	targetID, _ := strconv.ParseInt(targetIDStr, 10, 64)

	data := map[string]interface{}{
		"Title":      "新增风险备注",
		"BasePath":   h.getBasePath(),
		"Version":    h.version,
		"Nav":        "risks",
		"TargetType": targetType,
		"TargetID":   targetID,
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.riskFormTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render risk form template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// RiskEditForm renders the edit risk note form.
func (h *Handler) RiskEditForm(w http.ResponseWriter, r *http.Request) {
	idStr := r.URL.Query().Get("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil || id <= 0 {
		http.Error(w, "invalid risk note id", http.StatusBadRequest)
		return
	}
	riskNote, err := h.store.GetRiskNote(id)
	if err != nil {
		h.logger.Error("GetRiskNote failed", "error", err)
		http.Error(w, "risk note not found", http.StatusNotFound)
		return
	}
	targetType := riskNote.TargetType
	var targetID int64
	if riskNote.TargetID != nil {
		targetID = *riskNote.TargetID
	}
	data := map[string]interface{}{
		"Title":      "编辑风险备注",
		"BasePath":   h.getBasePath(),
		"Version":    h.version,
		"Nav":        "risks",
		"RiskNote":   riskNote,
		"TargetType": targetType,
		"TargetID":   targetID,
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.riskFormTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render risk form template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// RiskSave handles risk note create/update.
func (h *Handler) RiskSave(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		respondError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}
	if err := r.ParseForm(); err != nil {
		respondError(w, http.StatusBadRequest, "failed to parse form")
		return
	}

	var targetID *int64
	if tidStr := r.FormValue("target_id"); tidStr != "" {
		if tid, err := strconv.ParseInt(tidStr, 10, 64); err == nil && tid > 0 {
			targetID = &tid
		}
	}

	rn := &store.RiskNote{
		TargetType: r.FormValue("target_type"),
		TargetID:   targetID,
		Level:      r.FormValue("level"),
		Title:      r.FormValue("title"),
		Content:    r.FormValue("content"),
		Source:     r.FormValue("source"),
		ExpiresAt:  r.FormValue("expires_at"),
		ResolvedAt: r.FormValue("resolved_at"),
	}

	idStr := r.FormValue("id")
	if idStr != "" {
		id, err := strconv.ParseInt(idStr, 10, 64)
		if err != nil {
			respondError(w, http.StatusBadRequest, "invalid id")
			return
		}
		existing, err := h.store.GetRiskNote(id)
		if err != nil {
			h.logger.Error("GetRiskNote failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to get risk note")
			return
		}
		rn.ID = existing.ID
		rn.CreatedAt = existing.CreatedAt
		if err := h.store.UpdateRiskNote(rn); err != nil {
			h.logger.Error("UpdateRiskNote failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to update risk note")
			return
		}
	} else {
		if _, err := h.store.InsertRiskNote(rn); err != nil {
			h.logger.Error("InsertRiskNote failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to create risk note")
			return
		}
	}

	respondJSON(w, http.StatusOK, map[string]string{"redirect": h.getBasePath() + "/qb/risks"})
}

// RiskDelete handles risk note deletion.
func (h *Handler) RiskDelete(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		respondError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}
	idStr := r.URL.Query().Get("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil || id <= 0 {
		respondError(w, http.StatusBadRequest, "invalid risk note id")
		return
	}
	if err := h.store.DeleteRiskNote(id); err != nil {
		h.logger.Error("DeleteRiskNote failed", "error", err)
		respondError(w, http.StatusInternalServerError, "failed to delete risk note")
		return
	}
	respondJSON(w, http.StatusOK, map[string]string{"redirect": h.getBasePath() + "/qb/risks"})
}

// ---------------------------------------------------------------------------
// Model form handlers
// ---------------------------------------------------------------------------

// ModelNewForm renders the new model form.
func (h *Handler) ModelNewForm(w http.ResponseWriter, r *http.Request) {
	platformIDStr := r.URL.Query().Get("platform_id")
	platformID, err := strconv.ParseInt(platformIDStr, 10, 64)
	if err != nil || platformID <= 0 {
		http.Error(w, "invalid platform_id", http.StatusBadRequest)
		return
	}

	platforms, err := h.store.ListPlatforms()
	if err != nil {
		h.logger.Error("ListPlatforms failed", "error", err)
		platforms = nil
	}
	plans, err := h.store.ListPlans()
	if err != nil {
		h.logger.Error("ListPlans failed", "error", err)
		plans = nil
	}

	data := map[string]interface{}{
		"Title":     "新增模型",
		"BasePath":  h.getBasePath(),
		"Version":   h.version,
		"Nav":       "plans",
		"Platforms": platforms,
		"Plans":     plans,
		"Model":     nil,
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.modelFormTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render model form template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// ModelEditForm renders the edit model form.
func (h *Handler) ModelEditForm(w http.ResponseWriter, r *http.Request) {
	idStr := r.URL.Query().Get("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil || id <= 0 {
		http.Error(w, "invalid model id", http.StatusBadRequest)
		return
	}
	model, err := h.store.GetModel(id)
	if err != nil {
		h.logger.Error("GetModel failed", "error", err)
		http.Error(w, "model not found", http.StatusNotFound)
		return
	}

	platforms, err := h.store.ListPlatforms()
	if err != nil {
		h.logger.Error("ListPlatforms failed", "error", err)
		platforms = nil
	}
	plans, err := h.store.ListPlans()
	if err != nil {
		h.logger.Error("ListPlans failed", "error", err)
		plans = nil
	}

	var planIDValue int64
	if model.PlanID != nil {
		planIDValue = *model.PlanID
	}

	data := map[string]interface{}{
		"Title":     "编辑模型",
		"BasePath":  h.getBasePath(),
		"Version":   h.version,
		"Nav":       "plans",
		"Model":     model,
		"Platforms": platforms,
		"Plans":     plans,
		"PlanIDValue": planIDValue,
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := h.modelFormTmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		h.logger.Error("failed to render model form template", "error", err)
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
	}
}

// ModelSave handles model create/update.
func (h *Handler) ModelSave(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		respondError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}
	if err := r.ParseForm(); err != nil {
		respondError(w, http.StatusBadRequest, "failed to parse form")
		return
	}

	platformID, _ := strconv.ParseInt(r.FormValue("platform_id"), 10, 64)

	var planID *int64
	if pidStr := r.FormValue("plan_id"); pidStr != "" {
		if pid, err := strconv.ParseInt(pidStr, 10, 64); err == nil && pid > 0 {
			planID = &pid
		}
	}

	m := &store.Model{
		PlatformID:      platformID,
		PlanID:          planID,
		ModelID:         r.FormValue("model_id"),
		DisplayName:     r.FormValue("display_name"),
		Family:          r.FormValue("family"),
		IsCurrent:       r.FormValue("is_current") == "1",
		BaseURLOverride: r.FormValue("base_url_override"),
		Status:          r.FormValue("status"),
	}

	idStr := r.FormValue("id")
	if idStr != "" {
		id, err := strconv.ParseInt(idStr, 10, 64)
		if err != nil {
			respondError(w, http.StatusBadRequest, "invalid id")
			return
		}
		existing, err := h.store.GetModel(id)
		if err != nil {
			h.logger.Error("GetModel failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to get model")
			return
		}
		m.ID = existing.ID
		m.CreatedAt = existing.CreatedAt
		if err := h.store.UpdateModel(m); err != nil {
			h.logger.Error("UpdateModel failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to update model")
			return
		}
	} else {
		if _, err := h.store.InsertModel(m); err != nil {
			h.logger.Error("InsertModel failed", "error", err)
			respondError(w, http.StatusInternalServerError, "failed to create model")
			return
		}
	}

	respondJSON(w, http.StatusOK, map[string]string{"redirect": h.getBasePath() + "/qb/plans"})
}

// ModelDelete handles model deletion.
func (h *Handler) ModelDelete(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		respondError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}
	idStr := r.URL.Query().Get("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil || id <= 0 {
		respondError(w, http.StatusBadRequest, "invalid model id")
		return
	}
	if err := h.store.DeleteModel(id); err != nil {
		h.logger.Error("DeleteModel failed", "error", err)
		respondError(w, http.StatusInternalServerError, "failed to delete model")
		return
	}
	respondJSON(w, http.StatusOK, map[string]string{"redirect": h.getBasePath() + "/qb/plans"})
}