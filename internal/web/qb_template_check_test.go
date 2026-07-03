package web

import (
	"html/template"
	"testing"
)

func TestQBTemplatesParse(t *testing.T) {
	pages := []string{
		"templates/qb_overview.html",
		"templates/qb_plans.html",
		"templates/qb_providers.html",
		"templates/qb_usage.html",
		"templates/qb_config.html",
		"templates/qb_risks.html",
		"templates/qb_import_export.html",
		"templates/qb_platform_form.html",
		"templates/qb_plan_form.html",
		"templates/qb_bucket_form.html",
		"templates/qb_risk_form.html",
		"templates/qb_model_form.html",
	}
	for _, p := range pages {
		_, err := template.New("").ParseFS(templatesFS, "templates/layout.html", p)
		if err != nil {
			t.Fatalf("parse %s: %v", p, err)
		}
	}
}
