#include "wxHtmlEditWidget.h"
#include <wx/wx.h>
#include <wx/timer.h>
#include <wx/tglbtn.h>
#include <cstdlib>
#include <ctime>

// ============================================================
// Live Dashboard Demo
// ============================================================

static const wxString DASHBOARD_HTML = wxString::FromUTF8(
    "<style>"
    "  * { box-sizing: border-box; }"
    "  body { font-family: -apple-system, Helvetica, Arial, sans-serif; "
    "         font-size: 10pt; color: #1e293b; padding: 0; margin: 0;"
    "         background: #f0f4ff; }"

    // Header — gradient with big emoji
    "  .header { background: linear-gradient(135deg, #6366f1, #8b5cf6, #a855f7);"
    "            color: #ffffff; padding: 16px 24px; }"
    "  .header h1 { color: #ffffff; font-size: 18pt; margin: 0; }"
    "  .header .subtitle { color: #e0e7ff; font-size: 9pt; margin-top: 4px; }"

    // Stats row — flex with colored cards
    "  .stats { display: flex; gap: 14px; padding: 18px 24px; }"
    "  .stat-card { padding: 16px 20px; border-radius: 12px; "
    "               border: none; color: #ffffff;"
    "               display: flex; justify-content: space-between; align-items: flex-start;"
    "               box-shadow: 0 2px 8px rgba(0,0,0,0.12); }"
    "  .stat-card-cpu  { background: linear-gradient(135deg, #3b82f6, #2563eb); }"
    "  .stat-card-mem  { background: linear-gradient(135deg, #10b981, #059669); }"
    "  .stat-card-req  { background: linear-gradient(135deg, #f59e0b, #d97706); }"
    "  .stat-card-err  { background: linear-gradient(135deg, #ef4444, #dc2626); }"
    "  .stat-info { }"
    "  .stat-emoji { font-size: 36pt; }"
    "  .stat-label { font-size: 8pt; color: rgba(255,255,255,0.85);"
    "                font-weight: bold; text-transform: uppercase;"
    "                letter-spacing: 1px; }"
    "  .stat-value { font-size: 24pt; font-weight: bold; color: #ffffff;"
    "                margin: 2px 0; }"
    "  .stat-change { font-size: 8pt; color: rgba(255,255,255,0.8); }"

    // Content
    "  .content { padding: 0 24px 24px 24px; }"
    "  .section-title { font-size: 12pt; font-weight: bold; color: #4338ca;"
    "                   margin: 18px 0 10px 0; }"

    // Two-column grid
    "  .grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }"

    // Panels
    "  .panel { border: 1px solid #e0e7ff; border-radius: 12px;"
    "           background: #ffffff; overflow: hidden;"
    "           box-shadow: 0 1px 4px rgba(0,0,0,0.06); }"
    "  .panel-header { background: linear-gradient(135deg, #eef2ff, #e0e7ff);"
    "                  padding: 10px 16px; border-bottom: 1px solid #c7d2fe;"
    "                  font-weight: bold; font-size: 10pt; color: #4338ca; }"
    "  .panel-body { padding: 14px 16px; }"

    // Table
    "  table { width: 100%; border-collapse: collapse; }"
    "  th { text-align: left; padding: 10px 14px; font-size: 8pt;"
    "       color: #6366f1; background: #eef2ff; font-weight: bold;"
    "       text-transform: uppercase; letter-spacing: 1px;"
    "       border-bottom: 2px solid #c7d2fe; }"
    "  td { padding: 10px 14px; border-bottom: 1px solid #f0f4ff;"
    "       font-size: 10pt; color: #334155; }"

    // Badges
    "  .badge { padding: 3px 10px; border-radius: 12px; font-size: 8pt;"
    "           font-weight: bold; }"
    "  .badge-ok   { background: #dcfce7; color: #166534; }"
    "  .badge-warn { background: #fef9c3; color: #854d0e; }"
    "  .badge-err  { background: #fee2e2; color: #991b1b; }"

    // Progress
    "  .progress-track { background: #e2e8f0; border-radius: 6px;"
    "                    height: 10px; margin-top: 6px; }"
    "  .progress-fill  { border-radius: 6px; height: 10px; }"
    "  .progress-label { font-size: 9pt; color: #64748b; margin-top: 4px; }"

    // Chart
    "  .chart { font-family: monospace; font-size: 11pt; "
    "           padding: 12px 0; line-height: 1.4; }"
    "  .chart-cpu { color: #3b82f6; }"
    "  .chart-rps { color: #f59e0b; }"
    "  .chart-meta { font-family: -apple-system, Helvetica, sans-serif;"
    "                font-size: 9pt; color: #64748b; margin-top: 4px; }"

    // Feed
    "  .feed-item { padding: 10px 16px; border-bottom: 1px solid #f0f4ff;"
    "               font-size: 10pt; color: #334155; }"
    "  .feed-time { color: #6366f1; font-weight: bold; font-size: 9pt; }"

    // Alerts
    "  .alert { padding: 12px 16px; border-radius: 10px; margin: 8px 0;"
    "           font-size: 10pt; }"
    "  .alert-info { background: #eff6ff; border: 1px solid #93c5fd; color: #1e40af; }"
    "  .alert-warn { background: #fffbeb; border: 1px solid #fcd34d; color: #92400e; }"
    "  .alert-crit { background: #fef2f2; border: 1px solid #fca5a5; color: #991b1b; }"

    // ======= DARK MODE =======
    "  .dark body            { background: #0b1222; color: #e2e8f0; }"
    "  .dark .header         { background: linear-gradient(135deg, #312e81, #1e1b4b); }"
    "  .dark .stat-card-cpu  { background: linear-gradient(135deg, #1d4ed8, #1e3a8a); }"
    "  .dark .stat-card-mem  { background: linear-gradient(135deg, #047857, #064e3b); }"
    "  .dark .stat-card-req  { background: linear-gradient(135deg, #b45309, #78350f); }"
    "  .dark .stat-card-err  { background: linear-gradient(135deg, #b91c1c, #7f1d1d); }"
    "  .dark .section-title  { color: #a5b4fc; }"
    "  .dark .panel          { background: #1e293b; border-color: #334155;"
    "                          box-shadow: 0 2px 8px rgba(0,0,0,0.3); }"
    "  .dark .panel-header   { background: linear-gradient(135deg, #1e1b4b, #172554);"
    "                          border-color: #334155; color: #a5b4fc; }"
    "  .dark th              { background: #1e1b4b; color: #a5b4fc;"
    "                          border-color: #334155; }"
    "  .dark td              { color: #cbd5e1; border-color: #1e293b; }"
    "  .dark .chart-cpu      { color: #60a5fa; }"
    "  .dark .chart-rps      { color: #fbbf24; }"
    "  .dark .chart-meta     { color: #94a3b8; }"
    "  .dark .feed-item      { color: #cbd5e1; border-color: #334155; }"
    "  .dark .feed-time      { color: #818cf8; }"
    "  .dark .progress-track { background: #334155; }"
    "  .dark .progress-label { color: #94a3b8; }"
    "  .dark .alert-info     { background: #172554; border-color: #1e40af; color: #93c5fd; }"
    "  .dark .alert-warn     { background: #422006; border-color: #92400e; color: #fcd34d; }"
    "  .dark .alert-crit     { background: #450a0a; border-color: #991b1b; color: #fca5a5; }"

    // ======= COMPACT =======
    "  .compact .stat-card     { padding: 10px 14px; }"
    "  .compact .stat-value    { font-size: 16pt; }"
    "  .compact .stat-emoji    { font-size: 26pt; }"
    "  .compact .stats         { gap: 8px; padding: 10px 24px; }"
    "  .compact .feed-item     { padding: 6px 12px; font-size: 9pt; }"
    "  .compact td             { padding: 6px 10px; font-size: 9pt; }"
    "  .compact th             { padding: 6px 10px; }"
    "  .compact .chart         { font-size: 9pt; padding: 8px 0; }"
    "  .compact .section-title { font-size: 10pt; margin: 10px 0 6px 0; }"
    "  .compact .panel-body    { padding: 10px 14px; }"

    "  .hidden { display: none; }"
    "</style>"

    // ======== HEADER ========
    "<div class=\"header\">"
    "  <h1>🚀 System Dashboard</h1>"
    "  <div class=\"subtitle\" id=\"clock\">Uptime: 0d 0h 0m 0s</div>"
    "</div>"

    // ======== STATS (colored gradient cards) ========
    "<div class=\"stats\">"
    "  <div class=\"stat-card stat-card-cpu\">"
    "    <div class=\"stat-info\">"
    "      <div class=\"stat-label\">CPU</div>"
    "      <div class=\"stat-value\" id=\"cpu-val\">0%</div>"
    "      <div class=\"stat-change\" id=\"cpu-chg\">--</div>"
    "    </div>"
    "    <div class=\"stat-emoji\">💻</div>"
    "  </div>"
    "  <div class=\"stat-card stat-card-mem\">"
    "    <div class=\"stat-info\">"
    "      <div class=\"stat-label\">Memory</div>"
    "      <div class=\"stat-value\" id=\"mem-val\">0 MB</div>"
    "      <div class=\"stat-change\" id=\"mem-chg\">--</div>"
    "    </div>"
    "    <div class=\"stat-emoji\">🧠</div>"
    "  </div>"
    "  <div class=\"stat-card stat-card-req\">"
    "    <div class=\"stat-info\">"
    "      <div class=\"stat-label\">Req/s</div>"
    "      <div class=\"stat-value\" id=\"req-val\">0</div>"
    "      <div class=\"stat-change\" id=\"req-chg\">--</div>"
    "    </div>"
    "    <div class=\"stat-emoji\">⚡</div>"
    "  </div>"
    "  <div class=\"stat-card stat-card-err\">"
    "    <div class=\"stat-info\">"
    "      <div class=\"stat-label\">Errors</div>"
    "      <div class=\"stat-value\" id=\"err-val\">0%</div>"
    "      <div class=\"stat-change\" id=\"err-chg\">--</div>"
    "    </div>"
    "    <div class=\"stat-emoji\">🔥</div>"
    "  </div>"
    "</div>"

    // ======== CONTENT ========
    "<div class=\"content\">"

    // Grid row 1: services + memory
    "  <div class=\"grid-2\">"

    "    <div class=\"panel\">"
    "      <div class=\"panel-header\">🔧 Services</div>"
    "      <div class=\"panel-body\" style=\"padding: 0;\">"
    "        <table id=\"svc-table\">"
    "          <thead><tr><th>Service</th><th>Status</th><th>Latency</th></tr></thead>"
    "          <tbody id=\"svc-tbody\">"
    "            <tr id=\"svc-api\"><td>🌐 API Gateway</td>"
    "              <td><span class=\"badge badge-ok\" id=\"svc-api-badge\">HEALTHY</span></td>"
    "              <td id=\"svc-api-lat\">12ms</td></tr>"
    "            <tr id=\"svc-db\"><td>🗄 Database</td>"
    "              <td><span class=\"badge badge-ok\" id=\"svc-db-badge\">HEALTHY</span></td>"
    "              <td id=\"svc-db-lat\">3ms</td></tr>"
    "            <tr id=\"svc-cache\"><td>⚡ Cache</td>"
    "              <td><span class=\"badge badge-ok\" id=\"svc-cache-badge\">HEALTHY</span></td>"
    "              <td id=\"svc-cache-lat\">1ms</td></tr>"
    "            <tr id=\"svc-queue\"><td>📨 Msg Queue</td>"
    "              <td><span class=\"badge badge-ok\" id=\"svc-queue-badge\">HEALTHY</span></td>"
    "              <td id=\"svc-queue-lat\">5ms</td></tr>"
    "          </tbody>"
    "        </table>"
    "      </div>"
    "    </div>"

    "    <div class=\"panel\">"
    "      <div class=\"panel-header\">💾 Resources</div>"
    "      <div class=\"panel-body\">"
    "        <div class=\"stat-label\" style=\"color: #10b981;\">MEMORY</div>"
    "        <div style=\"font-size: 16pt; font-weight: bold; color: #10b981;\""
    "             id=\"mem-big\">0 / 8192 MB</div>"
    "        <div class=\"progress-track\">"
    "          <div class=\"progress-fill\" id=\"mem-bar\""
    "               style=\"width: 0%; background: #10b981;\"></div>"
    "        </div>"
    "        <div class=\"progress-label\" id=\"mem-label\">0%</div>"

    "        <div class=\"stat-label\" style=\"color: #8b5cf6; margin-top: 16px;\">DISK I/O</div>"
    "        <div style=\"font-size: 16pt; font-weight: bold; color: #8b5cf6;\""
    "             id=\"disk-val\">0 MB/s</div>"
    "        <div class=\"progress-track\">"
    "          <div class=\"progress-fill\" id=\"disk-bar\""
    "               style=\"width: 0%; background: #8b5cf6;\"></div>"
    "        </div>"

    "        <div class=\"stat-label\" style=\"color: #f59e0b; margin-top: 16px;\">NETWORK</div>"
    "        <div style=\"font-size: 16pt; font-weight: bold; color: #f59e0b;\""
    "             id=\"net-val\">0 Mbps</div>"
    "        <div class=\"progress-track\">"
    "          <div class=\"progress-fill\" id=\"net-bar\""
    "               style=\"width: 0%; background: #f59e0b;\"></div>"
    "        </div>"
    "      </div>"
    "    </div>"

    "  </div>"

    // Grid row 2: charts
    "  <div class=\"grid-2\" style=\"margin-top: 16px;\">"

    "    <div class=\"panel\">"
    "      <div class=\"panel-header\">📈 CPU History (60s)</div>"
    "      <div class=\"panel-body\">"
    "        <div class=\"chart chart-cpu\" id=\"cpu-chart\">...</div>"
    "        <div class=\"chart-meta\" id=\"cpu-meta\">--</div>"
    "      </div>"
    "    </div>"

    "    <div class=\"panel\">"
    "      <div class=\"panel-header\">📊 Requests/s (60s)</div>"
    "      <div class=\"panel-body\">"
    "        <div class=\"chart chart-rps\" id=\"rps-chart\">...</div>"
    "        <div class=\"chart-meta\" id=\"rps-meta\">--</div>"
    "      </div>"
    "    </div>"

    "  </div>"

    // Feed
    "  <div class=\"section-title\">📝 Activity Feed</div>"
    "  <div class=\"panel\">"
    "    <div id=\"activity-feed\">"
    "      <div class=\"feed-item\">"
    "        <span class=\"feed-time\">--:--:--</span> "
    "🟢 Dashboard initialized"
    "      </div>"
    "    </div>"
    "  </div>"

    "  <div id=\"alerts\"></div>"
    "</div>");

// ============================================================

enum {
  ID_TICK = wxID_HIGHEST + 1,
  ID_DARK_MODE, ID_COMPACT, ID_PAUSE, ID_CHAOS,
  ID_ADD_SVC, ID_CLEAR_ALERTS, ID_CLEAR_FEED, ID_SPEED,
};

class DashboardApp : public wxApp {
public:
  bool OnInit() override {
    std::srand((unsigned)std::time(nullptr));
    wxInitAllImageHandlers();

    auto *frame = new wxFrame(nullptr, wxID_ANY, "wxHtmlEdit Dashboard",
                              wxDefaultPosition, wxSize(840, 940));

    auto *panel = new wxPanel(frame);
    auto *sizer = new wxBoxSizer(wxVERTICAL);

    // ---- Toolbar — LIGHT background so text is always readable ----
    auto *toolbar = new wxPanel(panel);
    toolbar->SetBackgroundColour(wxColour(255, 255, 255));
    auto *tbSizer = new wxBoxSizer(wxHORIZONTAL);
    tbSizer->AddSpacer(10);

    auto mkToggle = [&](int id, const wxString &label) {
      auto *b = new wxToggleButton(toolbar, id, label);
      tbSizer->Add(b, 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);
      return b;
    };
    auto mkBtn = [&](int id, const wxString &label) {
      auto *b = new wxButton(toolbar, id, label);
      tbSizer->Add(b, 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);
      return b;
    };

    mkToggle(ID_DARK_MODE, "🌙 Dark");
    mkToggle(ID_COMPACT,   "📏 Compact");
    auto *btnPause = mkToggle(ID_PAUSE, "ⸯ Pause");
    mkToggle(ID_CHAOS,     "🔥 Chaos");
    tbSizer->AddSpacer(8);
    mkBtn(ID_ADD_SVC,      "➕ Service");
    mkBtn(ID_CLEAR_ALERTS, "🗑 Alerts");
    mkBtn(ID_CLEAR_FEED,   "🗑 Feed");
    tbSizer->AddStretchSpacer();

    auto *speedLabel = new wxStaticText(toolbar, wxID_ANY, "Speed:");
    tbSizer->Add(speedLabel, 0, wxALIGN_CENTER_VERTICAL | wxLEFT, 4);
    auto *speedSlider = new wxSlider(toolbar, ID_SPEED, 1000, 200, 3000,
                                     wxDefaultPosition, wxSize(90, -1));
    tbSizer->Add(speedSlider, 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);
    tbSizer->AddSpacer(10);

    toolbar->SetSizer(tbSizer);
    sizer->Add(toolbar, 0, wxEXPAND);

    auto *widget = new wxHtmlEditWidget(panel);
    widget->SetReadOnly(true);
    widget->SetHTML(DASHBOARD_HTML);
    sizer->Add(widget, 1, wxEXPAND);
    panel->SetSizer(sizer);

    // Auto-detect system dark mode — apply .dark class to HTML content
    bool systemDark = wxSystemSettings::GetAppearance().IsDark();
    auto *darkBtn = dynamic_cast<wxToggleButton *>(toolbar->FindWindow(ID_DARK_MODE));
    if (systemDark) {
      Box *root = widget->GetDocument().root.get();
      if (root) widget->AddClass(root, "dark");
      if (darkBtn) darkBtn->SetValue(true);
    }

    // ---- State ----
    struct State {
      int tick = 0;
      int cpu = 23, mem = 3200, rps = 1420, disk = 45, net = 120;
      float errRate = 0.12f;
      int prevCpu = 23, prevMem = 3200, prevRps = 1420;
      float prevErr = 0.12f;
      std::vector<int> cpuHistory, rpsHistory;
      int feedCount = 1, alertId = 0, svcCount = 4;
      bool chaosMode = false, paused = false;
      int baseLat[4] = {12, 3, 1, 5};
    };
    auto *st = new State();

    auto sparkline = [](const std::vector<int> &data, int maxVal) -> wxString {
      const wxChar bars[] = {0x2581,0x2582,0x2583,0x2584,0x2585,0x2586,0x2587,0x2588};
      wxString out;
      for (int v : data) {
        int i = (v * 7) / std::max(maxVal, 1);
        out += bars[std::max(0, std::min(7, i))];
      }
      return out;
    };
    auto timeStr = []() -> wxString {
      return wxDateTime::Now().Format("%H:%M:%S");
    };
    auto setChange = [widget](const char *id, int cur, int prev) {
      Box *el = widget->QuerySelector(id); if (!el) return;
      int d = cur - prev;
      widget->SetTextContent(el,
          d > 0 ? wxString::Format("+%d", d) :
          d < 0 ? wxString::Format("%d", d) : wxString("--"));
    };
    auto setChangeF = [widget](const char *id, float cur, float prev) {
      Box *el = widget->QuerySelector(id); if (!el) return;
      float d = cur - prev;
      widget->SetTextContent(el,
          d > 0.005f ? wxString::Format("+%.2f%%", d) :
          d < -0.005f ? wxString::Format("%.2f%%", d) : wxString("stable"));
    };

    // ---- Toolbar bindings ----
    toolbar->Bind(wxEVT_TOGGLEBUTTON, [widget](wxCommandEvent &e) {
      Box *root = widget->GetDocument().root.get();
      if (!root) return;
      if (e.IsChecked()) widget->AddClass(root, "dark");
      else widget->RemoveClass(root, "dark");
    }, ID_DARK_MODE);

    toolbar->Bind(wxEVT_TOGGLEBUTTON, [widget](wxCommandEvent &e) {
      Box *root = widget->GetDocument().root.get();
      if (!root) return;
      if (e.IsChecked()) widget->AddClass(root, "compact");
      else widget->RemoveClass(root, "compact");
    }, ID_COMPACT);

    btnPause->Bind(wxEVT_TOGGLEBUTTON, [st](wxCommandEvent &e) {
      st->paused = e.IsChecked();
    });

    toolbar->Bind(wxEVT_TOGGLEBUTTON, [st, widget, timeStr](wxCommandEvent &e) {
      st->chaosMode = e.IsChecked();
      if (e.IsChecked()) {
        Box *alerts = widget->QuerySelector("#alerts");
        if (alerts) {
          st->alertId++;
          Box *a = widget->CreateElement("div");
          widget->AddClass(a, "alert"); widget->AddClass(a, "alert-warn");
          widget->AppendChild(alerts, a);
          widget->SetTextContent(a, wxString::Format(
              "🔥 [%s] CHAOS MODE ENGAGED — brace yourself",
              timeStr()));
        }
      }
    }, ID_CHAOS);

    frame->Bind(wxEVT_BUTTON, [widget, st](wxCommandEvent &) {
      static const wxString names[] = {
        wxString::FromUTF8("🔴 Redis"), wxString::FromUTF8("📦 Kafka"),
        wxString::FromUTF8("🌐 Nginx"), wxString::FromUTF8("🔍 Elastic"),
        wxString::FromUTF8("📊 Prometheus"), wxString::FromUTF8("🔐 Vault"),
        wxString::FromUTF8("🗺 Consul"), wxString::FromUTF8("📉 Grafana"),
        wxString::FromUTF8("🔭 Jaeger"), wxString::FromUTF8("📁 MinIO")
      };
      st->svcCount++;
      int idx = (st->svcCount - 5) % 10;
      wxString rowId = wxString::Format("svc-x%d", st->svcCount);
      wxString badgeId = rowId + "-badge";
      wxString latId = rowId + "-lat";

      Box *tbody = widget->QuerySelector("#svc-tbody");
      if (!tbody) return;
      Box *row = widget->CreateElement("tr");
      widget->SetAttribute(row, "id", rowId);
      widget->AppendChild(tbody, row);
      widget->SetInnerHTML(row, wxString::Format(
          "<td>%s</td>"
          "<td><span class=\"badge badge-ok\" id=\"%s\">HEALTHY</span></td>"
          "<td id=\"%s\">%dms</td>",
          names[idx], badgeId, latId, 2 + std::rand() % 20));
    }, ID_ADD_SVC);

    frame->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      Box *a = widget->QuerySelector("#alerts"); if (!a) return;
      while (widget->GetChildCount(a) > 0) {
        Box *f = widget->GetFirstChild(a); if (f) widget->RemoveChild(f);
      }
    }, ID_CLEAR_ALERTS);

    frame->Bind(wxEVT_BUTTON, [widget, timeStr](wxCommandEvent &) {
      Box *feed = widget->QuerySelector("#activity-feed"); if (!feed) return;
      while (widget->GetChildCount(feed) > 0) {
        Box *f = widget->GetFirstChild(feed); if (f) widget->RemoveChild(f);
      }
      Box *item = widget->CreateElement("div");
      widget->AddClass(item, "feed-item");
      widget->AppendChild(feed, item);
      widget->SetTextContent(item,
          wxString::Format("%s  🧹 Feed cleared", timeStr()));
    }, ID_CLEAR_FEED);

    frame->SetClientData(reinterpret_cast<void *>(static_cast<intptr_t>(1000)));
    speedSlider->Bind(wxEVT_SLIDER, [speedSlider, frame](wxCommandEvent &) {
      frame->SetClientData(reinterpret_cast<void *>(
          static_cast<intptr_t>(speedSlider->GetValue())));
    });

    // ---- Main timer ----
    auto *timer = new wxTimer(frame, ID_TICK);
    frame->Bind(wxEVT_TIMER, [=](wxTimerEvent &) {
      if (st->paused) return;
      st->tick++;

      int curMs = static_cast<int>(reinterpret_cast<intptr_t>(frame->GetClientData()));
      if (timer->GetInterval() != curMs) timer->Start(curMs);

      st->prevCpu=st->cpu; st->prevMem=st->mem;
      st->prevRps=st->rps; st->prevErr=st->errRate;

      int sw = st->chaosMode ? 25 : 7;
      st->cpu = std::max(3, std::min(99, st->cpu + (std::rand()%(sw*2+1))-sw));
      int msw = st->chaosMode ? 500 : 90;
      st->mem = std::max(800, std::min(7900, st->mem + (std::rand()%(msw*2+1))-msw));
      int rsw = st->chaosMode ? 800 : 150;
      st->rps = std::max(100, std::min(6000, st->rps + (std::rand()%(rsw*2+1))-rsw));
      st->disk = std::max(5, std::min(500, st->disk + (std::rand()%61)-30));
      st->net = std::max(10, std::min(1000, st->net + (std::rand()%101)-50));
      float esw = st->chaosMode ? 0.5f : 0.05f;
      st->errRate = std::max(0.0f, std::min(8.0f,
          st->errRate + ((std::rand()%100)-50)*(esw/50.0f)));
      if (st->chaosMode && std::rand()%5==0) st->errRate += 1.0f;

      st->cpuHistory.push_back(st->cpu);
      if (st->cpuHistory.size()>60) st->cpuHistory.erase(st->cpuHistory.begin());
      st->rpsHistory.push_back(st->rps);
      if (st->rpsHistory.size()>60) st->rpsHistory.erase(st->rpsHistory.begin());

      // Stats
      Box *b;
      b=widget->QuerySelector("#cpu-val"); if(b) widget->SetTextContent(b,wxString::Format("%d%%",st->cpu));
      b=widget->QuerySelector("#mem-val"); if(b) widget->SetTextContent(b,wxString::Format("%d MB",st->mem));
      b=widget->QuerySelector("#req-val"); if(b) widget->SetTextContent(b,wxString::Format("%d",st->rps));
      b=widget->QuerySelector("#err-val"); if(b) widget->SetTextContent(b,wxString::Format("%.2f%%",st->errRate));
      setChange("#cpu-chg",st->cpu,st->prevCpu);
      setChange("#mem-chg",st->mem,st->prevMem);
      setChange("#req-chg",st->rps,st->prevRps);
      setChangeF("#err-chg",st->errRate,st->prevErr);

      // Clock
      b=widget->QuerySelector("#clock");
      if(b){int s=st->tick,d=s/86400;s%=86400;int h=s/3600;s%=3600;int m=s/60;s%=60;
        widget->SetTextContent(b,wxString::Format("Uptime: %dd %dh %dm %ds",d,h,m,s));}

      // Charts
      b=widget->QuerySelector("#cpu-chart");
      if(b) widget->SetTextContent(b,sparkline(st->cpuHistory,100));
      b=widget->QuerySelector("#cpu-meta");
      if(b){int mn=100,mx=0;for(int v:st->cpuHistory){mn=std::min(mn,v);mx=std::max(mx,v);}
        widget->SetTextContent(b,wxString::Format("min %d%%  max %d%%  now %d%%",mn,mx,st->cpu));}

      b=widget->QuerySelector("#rps-chart");
      if(b) widget->SetTextContent(b,sparkline(st->rpsHistory,6000));
      b=widget->QuerySelector("#rps-meta");
      if(b){int mn=99999,mx=0;for(int v:st->rpsHistory){mn=std::min(mn,v);mx=std::max(mx,v);}
        widget->SetTextContent(b,wxString::Format("min %d  max %d  now %d req/s",mn,mx,st->rps));}

      // Resources
      b=widget->QuerySelector("#mem-bar");
      if(b){int p=(st->mem*100)/8192;
        widget->SetStyleProperty(b,"width",wxString::Format("%d%%",p));
        widget->SetStyleProperty(b,"background",p>85?"#ef4444":p>65?"#f59e0b":"#10b981");}
      b=widget->QuerySelector("#mem-big");
      if(b) widget->SetTextContent(b,wxString::Format("%d / 8192 MB",st->mem));
      b=widget->QuerySelector("#mem-label");
      if(b) widget->SetTextContent(b,wxString::Format("%d%% used",(st->mem*100)/8192));
      b=widget->QuerySelector("#disk-val");
      if(b) widget->SetTextContent(b,wxString::Format("%d MB/s",st->disk));
      b=widget->QuerySelector("#disk-bar");
      if(b) widget->SetStyleProperty(b,"width",wxString::Format("%d%%",(st->disk*100)/500));
      b=widget->QuerySelector("#net-val");
      if(b) widget->SetTextContent(b,wxString::Format("%d Mbps",st->net));
      b=widget->QuerySelector("#net-bar");
      if(b) widget->SetStyleProperty(b,wxString("width"),wxString::Format("%d%%",(st->net*100)/1000));

      // Services table
      const char *svcs[]={"api","db","cache","queue"};
      const char *svcNames[]={"API Gateway","Database","Cache","Msg Queue"};
      for(int i=0;i<4;i++){
        wxString bid=wxString::Format("#svc-%s-badge",svcs[i]);
        wxString lid=wxString::Format("#svc-%s-lat",svcs[i]);
        Box *badge=widget->QuerySelector(bid);
        Box *lat=widget->QuerySelector(lid);
        int thr=st->chaosMode?20:3, wthr=st->chaosMode?35:8;
        int roll=std::rand()%100;
        int latency=st->baseLat[i]+(std::rand()%10);
        if(roll<thr){
          if(badge){widget->SetTextContent(badge,"DOWN");
            widget->RemoveClass(badge,"badge-ok");widget->RemoveClass(badge,"badge-warn");
            widget->AddClass(badge,"badge-err");}
          latency=999;
          Box *alerts=widget->QuerySelector("#alerts");
          if(alerts){st->alertId++;Box *a=widget->CreateElement("div");
            widget->AddClass(a,"alert");widget->AddClass(a,"alert-crit");
            widget->AppendChild(alerts,a);
            widget->SetTextContent(a,wxString::Format(
              "🚨 [%s] %s is DOWN!",timeStr(),svcNames[i]));}
        }else if(roll<wthr){
          if(badge){widget->SetTextContent(badge,"WARN");
            widget->RemoveClass(badge,"badge-ok");widget->RemoveClass(badge,"badge-err");
            widget->AddClass(badge,"badge-warn");}
          latency*=5;
        }else{
          if(badge){widget->SetTextContent(badge,"OK");
            widget->RemoveClass(badge,"badge-warn");widget->RemoveClass(badge,"badge-err");
            widget->AddClass(badge,"badge-ok");}
        }
        if(lat) widget->SetTextContent(lat,wxString::Format("%dms",latency));
      }
      // Custom services
      for(int i=5;i<=st->svcCount;i++){
        wxString bid=wxString::Format("#svc-x%d-badge",i);
        Box *badge=widget->QuerySelector(bid); if(!badge)continue;
        int roll=std::rand()%100;
        int thr=st->chaosMode?20:3,wthr=st->chaosMode?35:8;
        if(roll<thr){widget->SetTextContent(badge,"DOWN");
          widget->RemoveClass(badge,"badge-ok");widget->RemoveClass(badge,"badge-warn");
          widget->AddClass(badge,"badge-err");
        }else if(roll<wthr){widget->SetTextContent(badge,"WARN");
          widget->RemoveClass(badge,"badge-ok");widget->RemoveClass(badge,"badge-err");
          widget->AddClass(badge,"badge-warn");
        }else{widget->SetTextContent(badge,"OK");
          widget->RemoveClass(badge,"badge-warn");widget->RemoveClass(badge,"badge-err");
          widget->AddClass(badge,"badge-ok");}
        wxString lid=wxString::Format("#svc-x%d-lat",i);
        Box *lat=widget->QuerySelector(lid);
        if(lat) widget->SetTextContent(lat,wxString::Format("%dms",2+std::rand()%30));
      }

      // Feed
      if(st->tick%3==0){
        static const wxString events[]={
          wxString::FromUTF8("🚀 Deployment completed"),
          wxString::FromUTF8("📈 Auto-scaler: 3 -> 4 replicas"),
          wxString::FromUTF8("🔒 SSL certificate renewed"),
          wxString::FromUTF8("💾 DB backup done (2.3 GB)"),
          wxString::FromUTF8("⚡ Cache cleared /api/v2/*"),
          wxString::FromUTF8("🛡 Rate limit: 203.0.113.42"),
          wxString::FromUTF8("✅ Health check passed"),
          wxString::FromUTF8("🔄 Config reload: feature-flags"),
          wxString::FromUTF8("👤 New user #14,203"),
          wxString::FromUTF8("📡 Webhook delivered"),
          wxString::FromUTF8("📁 Log rotation: 340 MB archived"),
          wxString::FromUTF8("🧹 Job 'cleanup-sessions' done"),
          wxString::FromUTF8("🌍 CDN cache purged"),
          wxString::FromUTF8("⚠ Latency spike (p99: 340ms)"),
          wxString::FromUTF8("🐤 Canary: v2.4.1 at 10%%"),
          wxString::FromUTF8("🔌 Circuit breaker: payment-svc"),
        };
        Box *feed=widget->QuerySelector("#activity-feed");
        if(feed){
          while(widget->GetChildCount(feed)>=10){
            Box *f=widget->GetFirstChild(feed);if(f)widget->RemoveChild(f);}
          Box *item=widget->CreateElement("div");
          widget->AddClass(item,"feed-item");
          widget->AppendChild(feed,item);
          widget->SetTextContent(item,wxString::Format(
            "%s  %s",timeStr(),events[std::rand()%16]));
        }
      }

      // Alerts cleanup
      Box *alerts=widget->QuerySelector("#alerts");
      if(alerts&&widget->GetChildCount(alerts)>5){
        Box *f=widget->GetFirstChild(alerts);if(f)widget->RemoveChild(f);}
      if(st->errRate>3.0f&&st->tick%5==0&&alerts){
        st->alertId++;Box *a=widget->CreateElement("div");
        widget->AddClass(a,"alert");widget->AddClass(a,"alert-warn");
        widget->AppendChild(alerts,a);
        widget->SetTextContent(a,wxString::Format(
          "⚠ [%s] Error rate %.1f%% — threshold exceeded",
          timeStr(),st->errRate));}
    },ID_TICK);

    timer->Start(1000);
    frame->Bind(wxEVT_CLOSE_WINDOW,[timer,st](wxCloseEvent &e){
      timer->Stop();delete timer;delete st;e.Skip();});
    frame->Show();
    return true;
  }
};

wxIMPLEMENT_APP(DashboardApp);
