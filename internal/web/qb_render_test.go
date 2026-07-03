package web

import (
	"bytes"
	"html/template"
	"testing"

	"github.com/onllm-dev/onwatch/v2/internal/dashboard"
	"github.com/onllm-dev/onwatch/v2/internal/store"
)

func TestQBOverviewRendersWithSeed(t *testing.T) {
	s, err := store.New(":memory:")
	if err != nil {
		t.Fatalf("store: %v", err)
	}
	if err := dashboard.SeedSampleData(s); err != nil {
		t.Fatalf("seed: %v", err)
	}
	rec, err := dashboard.Recommend(s)
	if err != nil {
		t.Fatalf("recommend: %v", err)
	}
	platforms, err := s.ListPlatforms()
	if err != nil {
		t.Fatalf("list platforms: %v", err)
	}
	tmpl, err := template.New("").ParseFS(templatesFS, "templates/layout.html", "templates/qb_overview.html")
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	data := map[string]interface{}{
		"Title":     "模型额度看板",
		"BasePath":  "",
		"Version":   "test",
		"Nav":       "overview",
		"Rec":       rec,
		"Platforms": platforms,
	}
	var buf bytes.Buffer
	if err := tmpl.ExecuteTemplate(&buf, "layout.html", data); err != nil {
		t.Fatalf("execute: %v", err)
	}
	out := buf.String()
	for _, want := range []string{"火山方舟", "MiMo", "导入导出"} {
		if !bytes.Contains(buf.Bytes(), []byte(want)) {
			t.Errorf("overview render missing %q (len=%d)", want, len(out))
		}
	}
	t.Logf("overview rendered %d bytes", len(out))
}

func TestQBPlansRendersWithSeed(t *testing.T) {
	s, err := store.New(":memory:")
	if err != nil {
		t.Fatalf("store: %v", err)
	}
	if err := dashboard.SeedSampleData(s); err != nil {
		t.Fatalf("seed: %v", err)
	}
	platforms, _ := s.ListPlatforms()
	plans, _ := s.ListPlans()
	bucketsByPlan := map[int64][]store.QuotaBucket{}
	for _, p := range plans {
		bs, _ := s.ListQuotaBucketsByPlan(p.ID)
		bucketsByPlan[p.ID] = bs
	}
	tmpl, err := template.New("").ParseFS(templatesFS, "templates/layout.html", "templates/qb_plans.html")
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	data := map[string]interface{}{
		"Title":           "套餐管理",
		"BasePath":        "",
		"Version":         "test",
		"Nav":             "plans",
		"Platforms":       platforms,
		"Plans":           plans,
		"BucketsByPlan":   bucketsByPlan,
		"ModelsByPlatform": map[int64][]store.Model{},
	}
	var buf bytes.Buffer
	if err := tmpl.ExecuteTemplate(&buf, "layout.html", data); err != nil {
		t.Fatalf("execute: %v", err)
	}
	for _, want := range []string{"Coding Plan", "剩余"} {
		if !bytes.Contains(buf.Bytes(), []byte(want)) {
			t.Errorf("plans render missing %q (len=%d)", want, buf.Len())
		}
	}
	t.Logf("plans rendered %d bytes", buf.Len())
}
