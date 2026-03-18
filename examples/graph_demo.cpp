// graph_demo.cpp -- Demonstrates BOTH custom components AND event binding.
//
// Custom component: <graph> tag with measure+paint callbacks registered in C++.
// Event binding:    Click to cycle chart type, MouseEnter/Leave for hover
//                   highlights, Click on sidebar/KPIs for DOM manipulation,
//                   live event log, CSS :hover for visual effects.
//
// SECURITY: RegisterComponent() is app-side C++ only.

#include "wxHtmlEditWidget.h"
#include <wx/wx.h>
#include <wx/tokenzr.h>
#include <vector>
#include <algorithm>
#include <cmath>
#include <string>

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

// ============================================================
// Helpers
// ============================================================

static std::vector<wxString> SplitCSV(const wxString &s) {
    std::vector<wxString> out;
    wxStringTokenizer tok(s, ",");
    while (tok.HasMoreTokens())
        out.push_back(tok.GetNextToken().Trim(true).Trim(false));
    return out;
}

static wxColour ParseColor(const wxString &s) {
    if (s.IsEmpty()) return wxNullColour;
    wxColour c(s);
    return c.IsOk() ? c : wxNullColour;
}

static const wxColour kPalette[] = {
    wxColour( 78, 121, 167), wxColour(242, 142,  43),
    wxColour( 89, 161,  79), wxColour(225,  87,  89),
    wxColour(176, 122, 161), wxColour(255, 157, 167),
    wxColour(156, 117,  95), wxColour(186, 176,  52),
};
static const int kPaletteSize = 8;

static const char *kTypeList[] = {
    "bar", "line", "area", "pie", "donut", "hbar", "scatter", "gauge"
};
static const int kTypeCount = 8;

struct ChartData {
    wxString type, title;
    std::vector<double>   values;
    std::vector<wxString> labels;
    std::vector<wxColour> colors;
    double vmin = 0, vmax = 0;
    bool hasMin = false, hasMax = false;
};

static ChartData ReadChartData(const Box &box) {
    ChartData d;
    auto attr = [&](const char *key, const wxString &def = "") -> wxString {
        auto it = box.attributes.find(key);
        return (it != box.attributes.end()) ? wxString(it->second) : def;
    };
    d.type  = attr("data-type",  "bar");
    d.title = attr("data-title", "");
    d.hasMin = !attr("data-min").IsEmpty();
    d.hasMax = !attr("data-max").IsEmpty();
    if (d.hasMin) attr("data-min").ToDouble(&d.vmin);
    if (d.hasMax) attr("data-max").ToDouble(&d.vmax);
    for (auto &t : SplitCSV(attr("data-values"))) { double v; t.ToDouble(&v); d.values.push_back(v); }
    for (auto &t : SplitCSV(attr("data-labels"))) d.labels.push_back(t);
    for (auto &t : SplitCSV(attr("data-colors"))) d.colors.push_back(ParseColor(t));
    return d;
}

static wxColour GetColor(const ChartData &d, int i) {
    if (i < (int)d.colors.size() && d.colors[i].IsOk()) return d.colors[i];
    return kPalette[i % kPaletteSize];
}

static wxColour Darken(const wxColour &c, double f = 0.7) {
    return wxColour((unsigned char)(c.Red()*f), (unsigned char)(c.Green()*f), (unsigned char)(c.Blue()*f));
}

static wxColour WithAlpha(const wxColour &c, int a) {
    return wxColour(c.Red(), c.Green(), c.Blue(), a);
}

// ============================================================
// measure callback
// ============================================================

static wxSize GraphMeasure(const Box &box, int) {
    int w = 320, h = 200;
    { auto it = box.attributes.find("data-width");  if (it != box.attributes.end()) w = std::stoi(it->second); }
    { auto it = box.attributes.find("data-height"); if (it != box.attributes.end()) h = std::stoi(it->second); }
    return {w, h};
}

// ============================================================
// Drawing helpers
// ============================================================

struct PlotArea { int x, y, w, h; };

static PlotArea ComputePlotArea(const wxRect &cr, const ChartData &d) {
    return { cr.x + 44, cr.y + (d.title.IsEmpty() ? 10 : 28),
             cr.width - 54, cr.height - (d.title.IsEmpty() ? 10 : 28) - (d.labels.empty() ? 10 : 24) };
}

static void DrawBackground(wxDC &dc, const wxRect &cr) {
    dc.SetBrush(wxBrush(wxColour(22, 27, 34)));
    dc.SetPen(*wxTRANSPARENT_PEN);
    dc.DrawRectangle(cr);
}

static void DrawTitle(wxDC &dc, const wxRect &cr, const wxString &title) {
    if (title.IsEmpty()) return;
    wxFont tf = dc.GetFont(); tf.MakeBold(); dc.SetFont(tf);
    dc.SetTextForeground(wxColour(220, 225, 235));
    wxSize ts = dc.GetTextExtent(title);
    dc.DrawText(title, cr.x + (cr.width - ts.x) / 2, cr.y + 6);
    dc.SetFont(wxNullFont);
}

static void DrawAxes(wxDC &dc, const PlotArea &p, double vmin, double vmax, int ticks = 4) {
    wxFont sf = dc.GetFont();
    sf.SetPointSize(std::max(6, sf.GetPointSize() - 2));
    dc.SetFont(sf);
    dc.SetTextForeground(wxColour(100, 110, 130));
    for (int i = 0; i <= ticks; ++i) {
        double v = vmin + (vmax - vmin) * i / ticks;
        int gy = p.y + p.h - (int)(p.h * i / ticks);
        dc.SetPen(wxPen(wxColour(40, 45, 55)));
        dc.DrawLine(p.x, gy, p.x + p.w, gy);
        wxString lbl = (v == (int)v) ? wxString::Format("%d", (int)v) : wxString::Format("%.1f", v);
        wxSize ls = dc.GetTextExtent(lbl);
        dc.DrawText(lbl, p.x - ls.x - 4, gy - ls.y / 2);
    }
    dc.SetPen(wxPen(wxColour(60, 70, 85)));
    dc.DrawLine(p.x, p.y, p.x, p.y + p.h);
    dc.DrawLine(p.x, p.y + p.h, p.x + p.w, p.y + p.h);
}

static void DrawXLabels(wxDC &dc, const PlotArea &p, const ChartData &d, int n, double gw) {
    if (d.labels.empty()) return;
    dc.SetTextForeground(wxColour(100, 110, 130));
    wxFont sf = dc.GetFont(); sf.SetPointSize(std::max(6, sf.GetPointSize() - 2)); dc.SetFont(sf);
    for (int i = 0; i < n && i < (int)d.labels.size(); ++i) {
        int cx = p.x + (int)(i * gw + gw / 2.0);
        wxSize ls = dc.GetTextExtent(d.labels[i]);
        dc.DrawText(d.labels[i], cx - ls.x / 2, p.y + p.h + 5);
    }
}

// ============================================================
// Chart painters
// ============================================================

static void PaintBar(wxDC &dc, const ChartData &d, const PlotArea &p) {
    double vmin = d.hasMin ? d.vmin : 0.0;
    double vmax = d.hasMax ? d.vmax : *std::max_element(d.values.begin(), d.values.end());
    if (vmax <= vmin) vmax = vmin + 1;
    int n = (int)d.values.size();
    DrawAxes(dc, p, vmin, vmax);
    double gw = (double)p.w / n;
    int bw = std::max(4, (int)(gw * 0.7));
    dc.SetPen(*wxTRANSPARENT_PEN);
    for (int i = 0; i < n; ++i) {
        int x = p.x + (int)(i * gw + (gw - bw) / 2.0);
        int yv = p.y + p.h - (int)(p.h * (d.values[i] - vmin) / (vmax - vmin));
        int h = p.y + p.h - yv; if (h < 0) { yv = p.y; h = 0; }
        dc.SetBrush(wxBrush(GetColor(d, i)));
        dc.DrawRectangle(x, yv, bw, h);
        dc.SetPen(wxPen(Darken(GetColor(d, i), 1.3))); dc.DrawLine(x, yv, x + bw, yv); dc.SetPen(*wxTRANSPARENT_PEN);
    }
    DrawXLabels(dc, p, d, n, gw);
}

static void PaintLine(wxDC &dc, const ChartData &d, const PlotArea &p, bool filled) {
    double vmin = d.hasMin ? d.vmin : 0.0;
    double vmax = d.hasMax ? d.vmax : *std::max_element(d.values.begin(), d.values.end());
    if (vmax <= vmin) vmax = vmin + 1;
    int n = (int)d.values.size();
    double step = (double)p.w / std::max(n - 1, 1);
    DrawAxes(dc, p, vmin, vmax);
    auto valToY = [&](double v) { return p.y + p.h - (int)(p.h * (v - vmin) / (vmax - vmin)); };
    wxColour lc = GetColor(d, 0);
    if (filled) {
        std::vector<wxPoint> poly; poly.push_back({p.x, p.y + p.h});
        for (int i = 0; i < n; ++i) poly.push_back({(int)(p.x + i * step), valToY(d.values[i])});
        poly.push_back({(int)(p.x + (n - 1) * step), p.y + p.h});
        dc.SetBrush(wxBrush(WithAlpha(lc, 50))); dc.SetPen(*wxTRANSPARENT_PEN);
        dc.DrawPolygon((int)poly.size(), poly.data());
    }
    dc.SetPen(wxPen(lc, 2));
    for (int i = 0; i + 1 < n; ++i)
        dc.DrawLine((int)(p.x + i * step), valToY(d.values[i]), (int)(p.x + (i+1) * step), valToY(d.values[i+1]));
    dc.SetBrush(wxBrush(lc)); dc.SetPen(wxPen(wxColour(22, 27, 34), 2));
    for (int i = 0; i < n; ++i) dc.DrawCircle((int)(p.x + i * step), valToY(d.values[i]), 4);
    DrawXLabels(dc, p, d, n, (double)p.w / n);
}

static void PaintPie(wxDC &dc, const ChartData &d, const wxRect &cr, bool donut) {
    int n = (int)d.values.size();
    double total = 0; for (auto v : d.values) total += std::abs(v);
    if (total <= 0) return;
    int cx = cr.x + cr.width / 2, cy = cr.y + cr.height / 2 + (d.title.IsEmpty() ? 0 : 10);
    int r = std::min(cr.width, cr.height) / 2 - 30; if (r < 20) return;
    int ir = donut ? (int)(r * 0.55) : 0;
    double sa = -90.0;
    for (int i = 0; i < n; ++i) {
        double sw = (d.values[i] / total) * 360.0;
        if (sw < 0.5) { sa += sw; continue; }
        dc.SetBrush(wxBrush(GetColor(d, i))); dc.SetPen(wxPen(wxColour(22, 27, 34), 2));
        std::vector<wxPoint> pts;
        if (!donut) pts.push_back({cx, cy});
        int st = std::max(3, (int)(sw / 2));
        for (int s = 0; s <= st; ++s) { double a = (sa + sw * s / st) * M_PI / 180.0; pts.push_back({cx + (int)(r * cos(a)), cy + (int)(r * sin(a))}); }
        if (donut) { for (int s = st; s >= 0; --s) { double a = (sa + sw * s / st) * M_PI / 180.0; pts.push_back({cx + (int)(ir * cos(a)), cy + (int)(ir * sin(a))}); } }
        dc.DrawPolygon((int)pts.size(), pts.data());
        if (i < (int)d.labels.size() && sw > 15) {
            double ma = (sa + sw / 2.0) * M_PI / 180.0;
            int lr = donut ? (r + ir) / 2 : (int)(r * 0.65);
            wxFont sf = dc.GetFont(); sf.SetPointSize(std::max(6, sf.GetPointSize() - 2)); dc.SetFont(sf);
            dc.SetTextForeground(*wxWHITE);
            wxSize ts = dc.GetTextExtent(d.labels[i]);
            dc.DrawText(d.labels[i], cx + (int)(lr * cos(ma)) - ts.x / 2, cy + (int)(lr * sin(ma)) - ts.y / 2);
        }
        sa += sw;
    }
}

static void PaintHBar(wxDC &dc, const ChartData &d, const PlotArea &p) {
    int n = (int)d.values.size();
    double vmax = d.hasMax ? d.vmax : *std::max_element(d.values.begin(), d.values.end());
    if (vmax <= 0) vmax = 1;
    double gh = (double)p.h / n; int bh = std::max(4, (int)(gh * 0.65));
    dc.SetPen(wxPen(wxColour(40, 45, 55)));
    for (int i = 0; i <= 4; ++i) { int gx = p.x + p.w * i / 4; dc.DrawLine(gx, p.y, gx, p.y + p.h); }
    dc.SetPen(*wxTRANSPARENT_PEN);
    wxFont sf = dc.GetFont(); sf.SetPointSize(std::max(6, sf.GetPointSize() - 2)); dc.SetFont(sf);
    for (int i = 0; i < n; ++i) {
        int y = p.y + (int)(i * gh + (gh - bh) / 2.0);
        int bw = (int)(p.w * (d.values[i] / vmax)); if (bw < 0) bw = 0;
        dc.SetBrush(wxBrush(GetColor(d, i))); dc.DrawRectangle(p.x, y, bw, bh);
        dc.SetTextForeground(wxColour(160, 170, 190));
        dc.DrawText(wxString::Format("%.4g", d.values[i]), p.x + bw + 4, y + (bh - dc.GetTextExtent("X").y) / 2);
        if (i < (int)d.labels.size()) {
            dc.SetTextForeground(wxColour(130, 140, 160));
            wxSize ls = dc.GetTextExtent(d.labels[i]);
            dc.DrawText(d.labels[i], p.x - ls.x - 6, y + (bh - ls.y) / 2);
        }
    }
}

static void PaintScatter(wxDC &dc, const ChartData &d, const PlotArea &p) {
    int n = (int)d.values.size();
    double vmin = d.hasMin ? d.vmin : *std::min_element(d.values.begin(), d.values.end());
    double vmax = d.hasMax ? d.vmax : *std::max_element(d.values.begin(), d.values.end());
    double m = (vmax - vmin) * 0.1; if (m < 1) m = 1; vmin -= m; vmax += m;
    DrawAxes(dc, p, vmin, vmax);
    double sx = (double)p.w / std::max(n - 1, 1);
    wxColour col = GetColor(d, 0);
    for (int i = 0; i < n; ++i) {
        int x = (int)(p.x + i * sx), y = p.y + p.h - (int)(p.h * (d.values[i] - vmin) / (vmax - vmin));
        dc.SetBrush(wxBrush(WithAlpha(col, 40))); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawCircle(x, y, 8);
        dc.SetBrush(wxBrush(col)); dc.SetPen(wxPen(Darken(col))); dc.DrawCircle(x, y, 4);
    }
    DrawXLabels(dc, p, d, n, (double)p.w / n);
}

static void PaintGauge(wxDC &dc, const ChartData &d, const wxRect &cr) {
    double val = d.values.empty() ? 0.0 : d.values[0];
    double lo = d.hasMin ? d.vmin : 0.0, hi = d.hasMax ? d.vmax : 100.0;
    double pct = (val - lo) / (hi - lo); if (pct < 0) pct = 0; if (pct > 1) pct = 1;
    int cx = cr.x + cr.width / 2, cy = cr.y + cr.height / 2 + 20;
    int r = std::min(cr.width, cr.height) / 2 - 30; if (r < 20) return;
    for (int i = 0; i < 60; ++i) {
        double a1 = (180.0 + 180.0 * i / 60) * M_PI / 180.0, a2 = (180.0 + 180.0 * (i + 1) / 60) * M_PI / 180.0;
        dc.SetPen(wxPen(wxColour(40, 45, 55), 10));
        dc.DrawLine(cx + (int)(r * cos(a1)), cy + (int)(r * sin(a1)), cx + (int)(r * cos(a2)), cy + (int)(r * sin(a2)));
    }
    wxColour ac = GetColor(d, 0); int seg = (int)(60 * pct);
    for (int i = 0; i < seg; ++i) {
        double a1 = (180.0 + 180.0 * i / 60) * M_PI / 180.0, a2 = (180.0 + 180.0 * (i + 1) / 60) * M_PI / 180.0;
        dc.SetPen(wxPen(ac, 10));
        dc.DrawLine(cx + (int)(r * cos(a1)), cy + (int)(r * sin(a1)), cx + (int)(r * cos(a2)), cy + (int)(r * sin(a2)));
    }
    wxFont bf = dc.GetFont(); bf.SetPointSize(bf.GetPointSize() + 6); bf.MakeBold(); dc.SetFont(bf);
    dc.SetTextForeground(wxColour(230, 235, 245));
    wxString vt = wxString::Format("%.0f%%", pct * 100); wxSize vs = dc.GetTextExtent(vt);
    dc.DrawText(vt, cx - vs.x / 2, cy - vs.y - 4);
    if (!d.labels.empty()) {
        wxFont sf = dc.GetFont(); sf.SetPointSize(std::max(7, sf.GetPointSize() - 4)); sf.SetWeight(wxFONTWEIGHT_NORMAL); dc.SetFont(sf);
        dc.SetTextForeground(wxColour(120, 130, 150)); wxSize ls = dc.GetTextExtent(d.labels[0]);
        dc.DrawText(d.labels[0], cx - ls.x / 2, cy + 6);
    }
}

// ============================================================
// Main paint dispatcher
// ============================================================

static void GraphPaint(wxDC &dc, const Box &box, const wxRect &cr) {
    ChartData d = ReadChartData(box);
    if (d.values.empty()) {
        DrawBackground(dc, cr);
        dc.SetTextForeground(wxColour(80, 90, 110));
        dc.DrawText("(no data)", cr.x + cr.width / 2 - 30, cr.y + cr.height / 2 - 8);
        return;
    }
    DrawBackground(dc, cr);
    DrawTitle(dc, cr, d.title);

    if      (d.type == "pie")     PaintPie(dc, d, cr, false);
    else if (d.type == "donut")   PaintPie(dc, d, cr, true);
    else if (d.type == "hbar")  { PlotArea p = ComputePlotArea(cr, d); PaintHBar(dc, d, p); }
    else if (d.type == "scatter"){ PlotArea p = ComputePlotArea(cr, d); PaintScatter(dc, d, p); }
    else if (d.type == "area")  { PlotArea p = ComputePlotArea(cr, d); PaintLine(dc, d, p, true); }
    else if (d.type == "gauge")   PaintGauge(dc, d, cr);
    else if (d.type == "line")  { PlotArea p = ComputePlotArea(cr, d); PaintLine(dc, d, p, false); }
    else                        { PlotArea p = ComputePlotArea(cr, d); PaintBar(dc, d, p); }

    // Type badge in top-left
    wxFont hf = dc.GetFont(); hf.SetPointSize(std::max(6, hf.GetPointSize() - 2)); dc.SetFont(hf);
    dc.SetTextForeground(wxColour(88, 166, 255));
    dc.SetBrush(wxBrush(wxColour(15, 40, 70))); dc.SetPen(wxPen(wxColour(48, 80, 120)));
    wxString badge = d.type.Upper();
    wxSize bs = dc.GetTextExtent(badge);
    dc.DrawRoundedRectangle(cr.x + 6, cr.y + cr.height - bs.y - 10, bs.x + 10, bs.y + 4, 3);
    dc.DrawText(badge, cr.x + 11, cr.y + cr.height - bs.y - 8);
}

// ============================================================
// Demo HTML
// ============================================================

static const wxString kDemoHTML = wxString::FromUTF8(R"HTML(
<html>
<head><style>
* { box-sizing: border-box; }
body {
  font-family: -apple-system, Helvetica, Arial, sans-serif;
  font-size: 10pt; color: #c9d1d9; margin: 0; padding: 0;
  background: #0d1117;
}

/* ---- Header ---- */
.hdr {
  background: #161b22; border-bottom: 1px solid #30363d;
  padding: 14px 20px;
}
.hdr h1 { color: #f0f6fc; font-size: 14pt; margin: 0 0 2px 0; }
.hdr .sub { color: #8b949e; font-size: 8pt; margin: 0; }
.hdr .hint { color: #8b949e; font-size: 7pt; margin-top: 4px; }
.hdr .key {
  background: #21262d; color: #8b949e; padding: 1px 5px;
  border-radius: 3px; border: 1px solid #30363d; font-family: monospace;
}

/* ---- Status banner ---- */
.banner {
  background: #0c2d6b; color: #a5d6ff; font-size: 8pt;
  padding: 6px 20px; border-bottom: 1px solid #1f6feb;
  font-weight: 600;
}
.banner .ev-type { color: #58a6ff; }
.banner .ev-detail { color: #79c0ff; }

/* ---- Main 2-col layout ---- */
.main { display: flex; }
.sidebar {
  background: #161b22; border-right: 1px solid #30363d;
  padding: 14px; width: 170px; min-width: 170px;
}
.sidebar h3 { color: #f0f6fc; font-size: 9pt; margin: 0 0 8px 0; }
.sb-item {
  padding: 5px 8px; margin-bottom: 2px; border-radius: 6px;
  font-size: 8pt; color: #c9d1d9; cursor: pointer;
  border: 1px solid transparent;
}
.sb-item:hover { background: #21262d; border-color: #30363d; }
.sb-item-active { background: #1f6feb !important; color: #ffffff !important; border-color: #58a6ff !important; }
.sb-item .sstat { color: #8b949e; float: right; }
.sb-item-active .sstat { color: #a5d6ff !important; }
.sb-sep { height: 1px; background: #21262d; margin: 10px 0; }
.sb-dot {
  float: left; width: 6px; height: 6px;
  border-radius: 3px; margin-right: 5px; margin-top: 3px;
}
.sb-section { margin-top: 12px; }

.content { flex: 1; }

/* ---- KPI strip ---- */
.kpis { display: flex; gap: 10px; padding: 12px 16px; }
.kpi {
  background: #161b22; border: 1px solid #30363d; border-radius: 8px;
  padding: 10px 14px; flex: 1; cursor: pointer;
}
.kpi:hover { border-color: #58a6ff; background: #0d1824; }
.kpi-selected { border-color: #3fb950 !important; background: #0d2818 !important; }
.kpi .kl { font-size: 7pt; color: #8b949e; margin-bottom: 2px; }
.kpi .kv { font-size: 16pt; font-weight: bold; color: #f0f6fc; }
.kpi .kd { font-size: 7pt; margin-top: 2px; }
.kup { color: #3fb950; }
.kdn { color: #f85149; }

/* ---- Chart grid: 2 columns via table ---- */
.charts { padding: 4px 16px 16px 16px; }
.charts table { width: 100%; border-spacing: 12px; }
.charts td { vertical-align: top; }
.card {
  background: #161b22; border: 2px solid #444c56; border-radius: 10px;
  padding: 8px; overflow: hidden;
}
.card-glow { border-color: #58a6ff !important; background: #0d1824 !important; }
.card-clicked { border-color: #3fb950 !important; background: #0d2818 !important; }
.card .cap { font-size: 7pt; color: #8b949e; margin-top: 4px; padding: 0 4px; }
graph { display: block; cursor: pointer; }

/* ---- Event log ---- */
.log-wrap {
  margin: 0 16px 12px 16px; background: #161b22;
  border: 1px solid #21262d; border-radius: 8px;
  padding: 8px 12px; max-height: 110px; overflow: hidden;
}
.log-wrap h4 { color: #8b949e; font-size: 7pt; margin: 0 0 6px 0; text-transform: uppercase; letter-spacing: 1px; }
.log-line {
  font-family: monospace; font-size: 7pt; color: #8b949e;
  padding: 1px 0;
}
.log-line .lt { color: #58a6ff; font-weight: bold; }
.log-line .ltgt { color: #d2a8ff; }
.log-line .lval { color: #7ee787; }

/* ---- Footer ---- */
.ftr {
  border-top: 1px solid #21262d; padding: 10px 20px;
  font-size: 7pt; color: #8b949e;
}
.ftr code { color: #58a6ff; background: #161b22; padding: 1px 4px; border-radius: 3px; }

/* ---- Buttons ---- */
.btn-row { display: flex; gap: 8px; padding: 0 16px 12px 16px; }
.btn {
  padding: 5px 12px; border-radius: 6px; font-size: 8pt;
  font-weight: 600; cursor: pointer; border: none;
}
.btn:hover { opacity: 0.85; }
.btn-blue { background: #1f6feb; color: #fff; }
.btn-green { background: #3fb950; color: #fff; }
.btn-purple { background: #8b5cf6; color: #fff; }
.btn-orange { background: #d29922; color: #0d1117; }
.btn-red { background: #f85149; color: #fff; }
.btn-rand { background: #8b949e; color: #0d1117; }
.btn-grow { background: #238636; color: #fff; }
.btn-shrink { background: #da3633; color: #fff; }

/* ---- Counter row ---- */
.counter-row {
  padding: 0 16px 10px 16px; font-size: 8pt;
}
.counter-label { color: #8b949e; }
.counter-val {
  color: #58a6ff; font-weight: bold; font-size: 11pt;
  margin-left: 4px; margin-right: 4px;
}

</style></head>
<body>

<!-- Header -->
<div class="hdr">
  <h1>Custom Components + Events Demo</h1>
  <p class="sub">Showcases <code>RegisterComponent()</code> for the &lt;graph&gt; tag
     and <code>AddEventListener()</code> for interactivity</p>
  <p class="hint">
    <span class="key">Click</span> chart to cycle type
    <span class="key">Hover</span> chart for glow
    <span class="key">Click</span> sidebar/KPI for selection
    <span class="key">Click</span> buttons to change all charts
  </p>
</div>

<!-- Live status banner updated via SetTextContent -->
<div class="banner" id="status-banner">
  <span class="ev-type">Ready</span> &mdash;
  <span class="ev-detail" id="status-text">Interact with any element to see events fire</span>
</div>

<div class="main">

  <!-- Sidebar -->
  <div class="sidebar">
    <h3>Pages</h3>
    <div class="sb-item sb-item-active" id="sb-home">/home <span class="sstat">4,231</span></div>
    <div class="sb-item" id="sb-products">/products <span class="sstat">2,847</span></div>
    <div class="sb-item" id="sb-pricing">/pricing <span class="sstat">1,923</span></div>
    <div class="sb-item" id="sb-blog">/blog <span class="sstat">1,456</span></div>
    <div class="sb-item" id="sb-docs">/docs <span class="sstat">1,102</span></div>
    <div class="sb-item" id="sb-about">/about <span class="sstat">879</span></div>

    <div class="sb-sep"></div>
    <h3>Sources</h3>
    <div class="sb-item" id="sb-organic">
      <span class="sb-dot" style="background:#4e79a7;"></span>Organic <span class="sstat">48%</span>
    </div>
    <div class="sb-item" id="sb-direct">
      <span class="sb-dot" style="background:#f28e2b;"></span>Direct <span class="sstat">27%</span>
    </div>
    <div class="sb-item" id="sb-referral">
      <span class="sb-dot" style="background:#59a14f;"></span>Referral <span class="sstat">15%</span>
    </div>
    <div class="sb-item" id="sb-social">
      <span class="sb-dot" style="background:#e15759;"></span>Social <span class="sstat">10%</span>
    </div>

    <div class="sb-sep"></div>
    <h3>Devices</h3>
    <div class="sb-item" id="sb-desktop">Desktop <span class="sstat">62%</span></div>
    <div class="sb-item" id="sb-mobile">Mobile <span class="sstat">31%</span></div>
    <div class="sb-item" id="sb-tablet">Tablet <span class="sstat">7%</span></div>
  </div>

  <div class="content">

    <!-- KPI cards -->
    <div class="kpis">
      <div class="kpi" id="kpi-users">
        <div class="kl">Total Users</div>
        <div class="kv">12,438</div>
        <div class="kd kup">+12.5%</div>
      </div>
      <div class="kpi" id="kpi-views">
        <div class="kl">Page Views</div>
        <div class="kv">48,291</div>
        <div class="kd kup">+8.3%</div>
      </div>
      <div class="kpi" id="kpi-bounce">
        <div class="kl">Bounce Rate</div>
        <div class="kv">34.2%</div>
        <div class="kd kdn">+2.1%</div>
      </div>
      <div class="kpi" id="kpi-session">
        <div class="kl">Avg Session</div>
        <div class="kv">3m 42s</div>
        <div class="kd kup">+0.8%</div>
      </div>
    </div>

    <!-- Buttons row -->
    <div class="btn-row">
      <div class="btn btn-blue" id="btn-bar">All Bar</div>
      <div class="btn btn-green" id="btn-line">All Line</div>
      <div class="btn btn-purple" id="btn-pie">All Pie</div>
      <div class="btn btn-orange" id="btn-area">All Area</div>
      <div class="btn btn-red" id="btn-scatter">All Scatter</div>
      <div class="btn btn-rand" id="btn-rand">Randomize Data</div>
      <div class="btn btn-grow" id="btn-grow">Grow +10%</div>
      <div class="btn btn-shrink" id="btn-shrink">Shrink -10%</div>
    </div>

    <!-- Click counter -->
    <div class="counter-row">
      <span class="counter-label">Total interactions:</span>
      <span class="counter-val" id="click-count">0</span>
      <span class="counter-label" style="margin-left:20px;">Charts cycled:</span>
      <span class="counter-val" id="cycle-count">0</span>
      <span class="counter-label" style="margin-left:20px;">Data refreshes:</span>
      <span class="counter-val" id="refresh-count">0</span>
    </div>

    <!-- Charts in 2-column table -->
    <div class="charts">
      <table>
        <tr>
          <td><div class="card" id="card1">
            <graph id="g1"
              data-title="Weekly Revenue ($K)" data-type="bar"
              data-values="42,58,35,67,49,73,61"
              data-labels="Mon,Tue,Wed,Thu,Fri,Sat,Sun"
              data-colors="#4e79a7,#4e79a7,#4e79a7,#4e79a7,#4e79a7,#58a6ff,#58a6ff"
              data-width="340" data-height="190">
            </graph>
            <div class="cap">Revenue by day</div>
          </div></td>
          <td><div class="card" id="card2">
            <graph id="g2"
              data-title="Active Users (7d)" data-type="area"
              data-values="820,934,901,1034,1290,1330,1280"
              data-labels="Mon,Tue,Wed,Thu,Fri,Sat,Sun"
              data-colors="#3fb950"
              data-width="340" data-height="190">
            </graph>
            <div class="cap">User engagement trend</div>
          </div></td>
        </tr>
        <tr>
          <td><div class="card" id="card3">
            <graph id="g3"
              data-title="Traffic Sources" data-type="donut"
              data-values="48,27,15,10"
              data-labels="Organic,Direct,Referral,Social"
              data-colors="#4e79a7,#f28e2b,#59a14f,#e15759"
              data-width="340" data-height="190">
            </graph>
            <div class="cap">Source breakdown</div>
          </div></td>
          <td><div class="card" id="card4">
            <graph id="g4"
              data-title="Response Time (ms)" data-type="line"
              data-values="234,198,256,187,201,178,165,190,210,175,155,148"
              data-labels="J,F,M,A,M,J,J,A,S,O,N,D"
              data-colors="#d29922"
              data-min="100" data-max="300"
              data-width="340" data-height="190">
            </graph>
            <div class="cap">P95 latency over 12 months</div>
          </div></td>
        </tr>
        <tr>
          <td><div class="card" id="card5">
            <graph id="g5"
              data-title="Feature Usage" data-type="hbar"
              data-values="89,72,65,51,43,28"
              data-labels="Search,Upload,Export,Share,Filter,API"
              data-colors="#b07aa1,#b07aa1,#b07aa1,#b07aa1,#b07aa1,#b07aa1"
              data-width="340" data-height="190">
            </graph>
            <div class="cap">Most-used features this quarter</div>
          </div></td>
          <td><div class="card" id="card6">
            <graph id="g6"
              data-title="Server Uptime" data-type="gauge"
              data-values="99.7"
              data-labels="SLA: 99.5%"
              data-colors="#3fb950"
              data-min="95" data-max="100"
              data-width="340" data-height="190">
            </graph>
            <div class="cap">30-day rolling availability</div>
          </div></td>
        </tr>
      </table>
    </div>

    <!-- Event log -->
    <div class="log-wrap">
      <h4>Event Log (live)</h4>
      <div class="log-line" id="log5">&nbsp;</div>
      <div class="log-line" id="log4">&nbsp;</div>
      <div class="log-line" id="log3">&nbsp;</div>
      <div class="log-line" id="log2">&nbsp;</div>
      <div class="log-line" id="log1"><span class="lt">INIT</span> Demo loaded. Interact with elements above.</div>
    </div>

  </div>
</div>

<div class="ftr">
  <code>RegisterComponent("graph", measure, paint)</code> binds the custom drawing.
  <code>AddEventListener(selector, type, callback)</code> binds all interactivity.
  Components are C++ app-side only; HTML cannot activate them.
</div>

</body>
</html>
)HTML");

// ============================================================
// Helper: find the graph Box that is a parent/self of the target
// ============================================================
static Box *FindGraphAncestor(Box *box) {
    while (box) {
        if (box->tag == "graph") return box;
        box = box->parent;
    }
    return nullptr;
}

// ============================================================
// Demo frame
// ============================================================

class GraphDemoFrame : public wxFrame {
public:
    GraphDemoFrame()
        : wxFrame(nullptr, wxID_ANY, "wxHtmlEdit - Components + Events Demo",
                  wxDefaultPosition, wxSize(1100, 860))
    {
        auto *panel = new wxPanel(this);
        auto *sizer = new wxBoxSizer(wxVERTICAL);

        m_html = new wxHtmlEditWidget(panel, wxID_ANY);
        m_html->SetReadOnly(true);

        // ---- CUSTOM COMPONENT: register <graph> ----
        m_html->RegisterComponent(
            "graph",
            [](const Box &box, int cw) -> wxSize { return GraphMeasure(box, cw); },
            [](wxDC &dc, const Box &box, const wxRect &cr) { GraphPaint(dc, box, cr); });

        m_html->SetHTML(kDemoHTML);

        // ---- EVENT: Click graph to cycle chart type ----
        m_html->AddEventListener("graph", HtmlEventType::Click,
            [this](HtmlEvent &evt) {
                Box *g = FindGraphAncestor(evt.target);
                if (!g) return;
                auto it = g->attributes.find("data-type");
                std::string cur = (it != g->attributes.end()) ? it->second : "bar";
                int idx = 0;
                for (int i = 0; i < kTypeCount; ++i) { if (cur == kTypeList[i]) { idx = i; break; } }
                idx = (idx + 1) % kTypeCount;
                m_html->SetAttribute(g, "data-type", kTypeList[idx]);
                wxString id = m_html->GetAttribute(g, "id");
                // Flash the parent card green
                Box *card = g->parent;
                while (card && !m_html->HasClass(card, "card")) card = card->parent;
                if (card) {
                    m_html->AddClass(card, "card-clicked");
                    Box *cap = m_html->QuerySelector(
                        wxString::Format("#%s .cap", m_html->GetAttribute(card, "id")));
                    if (cap)
                        m_html->SetTextContent(cap, wxString::Format(
                            "Now showing: %s (click to cycle)", kTypeList[idx]));
                }
                ++m_cycleCount;
                BumpInteraction();
                UpdateStatus("Click", wxString::Format("%s -> %s", id, kTypeList[idx]));
                LogEvent("CLICK", wxString::Format("graph#%s -> %s", id, kTypeList[idx]));
                m_html->RequestLayout();
            });

        // ---- EVENT: MouseEnter/Leave card -> glow ----
        m_html->AddEventListener(".card", HtmlEventType::MouseEnter,
            [this](HtmlEvent &evt) {
                Box *card = evt.target;
                while (card && !m_html->HasClass(card, "card")) card = card->parent;
                if (!card) return;
                m_html->AddClass(card, "card-glow");
            });
        m_html->AddEventListener(".card", HtmlEventType::MouseLeave,
            [this](HtmlEvent &evt) {
                Box *card = evt.target;
                while (card && !m_html->HasClass(card, "card")) card = card->parent;
                if (!card) return;
                m_html->RemoveClass(card, "card-glow");
                m_html->RemoveClass(card, "card-clicked");
            });

        // ---- EVENT: Click sidebar -> select + update chart data ----
        m_html->AddEventListener(".sb-item", HtmlEventType::Click,
            [this](HtmlEvent &evt) {
                Box *item = evt.target;
                while (item && !m_html->HasClass(item, "sb-item")) item = item->parent;
                if (!item) return;
                auto all = m_html->QuerySelectorAll(".sb-item");
                for (auto *b : all) m_html->RemoveClass(b, "sb-item-active");
                m_html->AddClass(item, "sb-item-active");
                wxString id = m_html->GetAttribute(item, "id");
                BumpInteraction();
                // Multiply chart data based on which page is selected
                // This gives a real "data changes when you navigate" feel
                double mult = 1.0;
                if (id == "sb-home") mult = 1.0;
                else if (id == "sb-products") mult = 0.75;
                else if (id == "sb-pricing") mult = 0.5;
                else if (id == "sb-blog") mult = 0.4;
                else if (id == "sb-docs") mult = 0.3;
                else if (id == "sb-about") mult = 0.2;
                else if (id == "sb-organic") mult = 1.1;
                else if (id == "sb-direct") mult = 0.65;
                else if (id == "sb-referral") mult = 0.35;
                else if (id == "sb-social") mult = 0.25;
                else if (id == "sb-desktop") mult = 1.0;
                else if (id == "sb-mobile") mult = 0.6;
                else if (id == "sb-tablet") mult = 0.15;
                ScaleAllChartValues(mult);
                UpdateStatus("Navigate", wxString::Format("Showing data for %s", id));
                LogEvent("NAV", wxString::Format("%s  (scale=%.0f%%)", id, mult*100));
            });

        // ---- EVENT: Click KPI -> toggle + update the value text ----
        m_html->AddEventListener(".kpi", HtmlEventType::Click,
            [this](HtmlEvent &evt) {
                Box *kpi = evt.target;
                while (kpi && !m_html->HasClass(kpi, "kpi")) kpi = kpi->parent;
                if (!kpi) return;
                m_html->ToggleClass(kpi, "kpi-selected");
                wxString id = m_html->GetAttribute(kpi, "id");
                bool sel = m_html->HasClass(kpi, "kpi-selected");
                BumpInteraction();
                // When selected, bump the value; when deselected, restore
                Box *kv = m_html->QuerySelector(wxString::Format("#%s .kv", id));
                if (kv) {
                    if (id == "kpi-users")
                        m_html->SetTextContent(kv, sel ? "13,682" : "12,438");
                    else if (id == "kpi-views")
                        m_html->SetTextContent(kv, sel ? "53,120" : "48,291");
                    else if (id == "kpi-bounce")
                        m_html->SetTextContent(kv, sel ? "29.8%" : "34.2%");
                    else if (id == "kpi-session")
                        m_html->SetTextContent(kv, sel ? "4m 15s" : "3m 42s");
                }
                Box *kd = m_html->QuerySelector(wxString::Format("#%s .kd", id));
                if (kd) {
                    if (sel) {
                        m_html->RemoveClass(kd, "kdn");
                        m_html->AddClass(kd, "kup");
                        m_html->SetTextContent(kd, "Filtered");
                    } else {
                        m_html->SetTextContent(kd, id == "kpi-bounce" ? "+2.1%" : (id == "kpi-session" ? "+0.8%" : (id == "kpi-views" ? "+8.3%" : "+12.5%")));
                    }
                }
                UpdateStatus("Click", wxString::Format("KPI %s %s", id, sel ? "ON" : "OFF"));
                LogEvent("CLICK", wxString::Format(".kpi#%s -> %s", id, sel ? "ON" : "OFF"));
            });

        // ---- EVENT: Buttons to set all charts ----
        auto setAllCharts = [this](const char *type) {
            auto graphs = m_html->QuerySelectorAll("graph");
            m_html->BeginBatchUpdate();
            for (auto *g : graphs)
                m_html->SetAttribute(g, "data-type", type);
            for (int i = 1; i <= 6; ++i) {
                Box *cap = m_html->QuerySelector(wxString::Format("#card%d .cap", i));
                if (cap) m_html->SetTextContent(cap, wxString::Format("All set to %s", type));
            }
            m_html->EndBatchUpdate();
            BumpInteraction();
            UpdateStatus("Click", wxString::Format("All charts -> %s", type));
            LogEvent("CLICK", wxString::Format("btn -> all = %s", type));
            m_html->RequestLayout();
        };

        m_html->AddEventListener("#btn-bar", HtmlEventType::Click,
            [this, setAllCharts](HtmlEvent &) { setAllCharts("bar"); });
        m_html->AddEventListener("#btn-line", HtmlEventType::Click,
            [this, setAllCharts](HtmlEvent &) { setAllCharts("line"); });
        m_html->AddEventListener("#btn-pie", HtmlEventType::Click,
            [this, setAllCharts](HtmlEvent &) { setAllCharts("pie"); });
        m_html->AddEventListener("#btn-area", HtmlEventType::Click,
            [this, setAllCharts](HtmlEvent &) { setAllCharts("area"); });
        m_html->AddEventListener("#btn-scatter", HtmlEventType::Click,
            [this, setAllCharts](HtmlEvent &) { setAllCharts("scatter"); });

        // ---- EVENT: Randomize data ----
        m_html->AddEventListener("#btn-rand", HtmlEventType::Click,
            [this](HtmlEvent &) {
                RandomizeAllCharts();
                ++m_refreshCount;
                BumpInteraction();
                UpdateStatus("Click", "All chart data randomized!");
                LogEvent("CLICK", "btn-rand -> randomized all data");
            });

        // ---- EVENT: Grow all values +10% ----
        m_html->AddEventListener("#btn-grow", HtmlEventType::Click,
            [this](HtmlEvent &) {
                ScaleAllChartValues(1.1);
                BumpInteraction();
                UpdateStatus("Click", "All values grew +10%");
                LogEvent("CLICK", "btn-grow -> +10%");
            });

        // ---- EVENT: Shrink all values -10% ----
        m_html->AddEventListener("#btn-shrink", HtmlEventType::Click,
            [this](HtmlEvent &) {
                ScaleAllChartValues(0.9);
                BumpInteraction();
                UpdateStatus("Click", "All values shrank -10%");
                LogEvent("CLICK", "btn-shrink -> -10%");
            });

        sizer->Add(m_html, 1, wxEXPAND);
        panel->SetSizer(sizer);

        CreateStatusBar();
        SetStatusText("Click charts, sidebar, KPIs, and buttons. Hover for effects. All via AddEventListener().");
    }

private:
    wxHtmlEditWidget *m_html = nullptr;
    int m_logCounter = 0;
    int m_interactionCount = 0;
    int m_cycleCount = 0;
    int m_refreshCount = 0;

    void BumpInteraction() {
        ++m_interactionCount;
        Box *c1 = m_html->QuerySelector("#click-count");
        if (c1) m_html->SetTextContent(c1, wxString::Format("%d", m_interactionCount));
        Box *c2 = m_html->QuerySelector("#cycle-count");
        if (c2) m_html->SetTextContent(c2, wxString::Format("%d", m_cycleCount));
        Box *c3 = m_html->QuerySelector("#refresh-count");
        if (c3) m_html->SetTextContent(c3, wxString::Format("%d", m_refreshCount));
    }

    void ScaleAllChartValues(double mult) {
        auto graphs = m_html->QuerySelectorAll("graph");
        m_html->BeginBatchUpdate();
        for (auto *g : graphs) {
            wxString vals = m_html->GetAttribute(g, "data-values");
            if (vals.IsEmpty()) continue;
            auto parts = SplitCSV(vals);
            wxString newVals;
            for (size_t i = 0; i < parts.size(); ++i) {
                double v; parts[i].ToDouble(&v);
                v *= mult;
                if (i > 0) newVals += ",";
                newVals += wxString::Format("%.0f", v);
            }
            m_html->SetAttribute(g, "data-values", newVals);
        }
        m_html->EndBatchUpdate();
        m_html->RequestLayout();
    }

    void RandomizeAllCharts() {
        auto graphs = m_html->QuerySelectorAll("graph");
        m_html->BeginBatchUpdate();
        for (auto *g : graphs) {
            wxString vals = m_html->GetAttribute(g, "data-values");
            if (vals.IsEmpty()) continue;
            auto parts = SplitCSV(vals);
            wxString newVals;
            for (size_t i = 0; i < parts.size(); ++i) {
                double v; parts[i].ToDouble(&v);
                // Random +-40% variation
                double factor = 0.6 + (rand() % 80) / 100.0;
                v = std::max(1.0, v * factor);
                if (i > 0) newVals += ",";
                newVals += wxString::Format("%.0f", v);
            }
            m_html->SetAttribute(g, "data-values", newVals);
        }
        m_html->EndBatchUpdate();
        m_html->RequestLayout();
    }

    void UpdateStatus(const wxString &evType, const wxString &detail) {
        Box *st = m_html->QuerySelector("#status-text");
        if (st) m_html->SetTextContent(st, detail);
        SetStatusText(wxString::Format("[%s] %s", evType, detail));
    }

    void LogEvent(const wxString &type, const wxString &detail) {
        ++m_logCounter;
        // Shift log lines down
        for (int i = 5; i >= 2; --i) {
            Box *dst = m_html->QuerySelector(wxString::Format("#log%d", i));
            Box *src = m_html->QuerySelector(wxString::Format("#log%d", i - 1));
            if (dst && src) {
                wxString html = m_html->GetInnerHTML(src);
                m_html->SetInnerHTML(dst, html);
            }
        }
        Box *log1 = m_html->QuerySelector("#log1");
        if (log1) {
            wxString entry = wxString::Format(
                "<span class=\"lt\">%s</span> #%d <span class=\"ltgt\">%s</span>",
                type, m_logCounter, detail);
            m_html->SetInnerHTML(log1, entry);
        }
    }
};

// ============================================================
// wxApp
// ============================================================

class GraphDemoApp : public wxApp {
public:
    bool OnInit() override {
        auto *frame = new GraphDemoFrame();
        frame->Show(true);
        return true;
    }
};

wxIMPLEMENT_APP(GraphDemoApp);
