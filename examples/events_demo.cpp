#include "wxHtmlEditWidget.h"
#include <wx/wx.h>
#include <wx/timer.h>
#include <cstdlib>
#include <ctime>

// ============================================================
// Interactive Kanban Board Demo
// Showcases every event type: DragStart/Drag/Drop/DragEnter/DragLeave/DragOver/DragEnd,
// Click, DblClick, MouseEnter/Leave, MouseMove, ContextMenu, KeyDown, KeyUp, KeyPress,
// Focus/Blur, Scroll, SelectionChange
// ============================================================

static const wxString KANBAN_HTML = wxString::FromUTF8(R"(
<style>
  * { box-sizing: border-box; }
  body {
    font-family: -apple-system, Helvetica, Arial, sans-serif;
    font-size: 10pt; color: #c9d1d9; padding: 0; margin: 0;
    background: #0d1117;
  }

  /* ---- Header ---- */
  .header {
    background: linear-gradient(135deg, #161b22, #21262d);
    color: #f0f6fc; padding: 14px 24px;
    border-bottom: 1px solid #30363d;
  }
  .header h1 { color: #f0f6fc; font-size: 16pt; margin: 0; }
  .header .subtitle { color: #8b949e; font-size: 8pt; margin-top: 2px; }
  .header .keys { color: #484f58; font-size: 7pt; margin-top: 4px; }
  .header .key {
    background: #21262d; color: #8b949e; padding: 1px 5px;
    border-radius: 3px; border: 1px solid #30363d; font-family: monospace;
  }

  /* ---- Status bar ---- */
  .status-bar {
    background: #161b22; color: #484f58; font-size: 8pt;
    padding: 6px 24px; border-bottom: 1px solid #21262d;
  }
  .status-bar .count { color: #58a6ff; font-weight: bold; }
  .focus-dot {
    display: inline-block; width: 6px; height: 6px;
    border-radius: 3px; background: #30363d; margin-right: 4px;
  }
  .focus-dot-active { background: #3fb950 !important; }

  /* ---- Drag indicator banner ---- */
  .drag-banner {
    display: none; background: #1f6feb; color: #ffffff;
    padding: 8px 24px; font-size: 9pt; font-weight: 600;
    border-bottom: 2px solid #58a6ff;
  }
  .drag-banner-visible { display: block !important; }
  .drag-banner .drag-icon { margin-right: 6px; }
  .drag-banner .drag-target { color: #a5d6ff; margin-left: 8px; }

  /* ---- Drag ghost: follows cursor ---- */
  .drag-ghost {
    display: none; position: absolute;
    background: #1f6feb; color: #ffffff;
    border: 2px solid #58a6ff; border-radius: 8px;
    padding: 8px 14px; font-size: 9pt; font-weight: 600;
    width: 180px; opacity: 0.9;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    pointer-events: none;
  }
  .drag-ghost-visible { display: block !important; }
  .drag-ghost .ghost-tag {
    font-size: 7pt; color: #a5d6ff; margin-top: 4px;
  }

  /* ---- Board ---- */
  .board {
    display: flex; gap: 16px; padding: 20px 24px;
    align-items: flex-start;
  }

  /* ---- Columns ---- */
  .column {
    background: #161b22; border-radius: 12px;
    padding: 0; min-width: 220px; width: 220px;
    border: 1px solid #30363d;
    transition: border-color 0.15s;
  }
  .column-drop-active {
    border-color: #58a6ff !important;
    box-shadow: 0 0 16px rgba(88,166,255,0.15) !important;
  }
  .column-header {
    padding: 12px 16px; font-weight: bold; font-size: 9pt;
    text-transform: uppercase; letter-spacing: 1px;
    border-radius: 12px 12px 0 0;
    display: flex; justify-content: space-between; align-items: center;
  }
  .column-header-glow {
    box-shadow: 0 2px 12px rgba(88,166,255,0.3) !important;
  }
  .column-count {
    background: rgba(255,255,255,0.15); border-radius: 10px;
    padding: 2px 8px; font-size: 8pt;
  }
  .column-body { padding: 8px 12px; min-height: 60px; }

  .col-backlog .column-header { background: #30363d; color: #c9d1d9; }
  .col-todo .column-header { background: #1f6feb; color: #ffffff; }
  .col-progress .column-header { background: #d29922; color: #ffffff; }
  .col-review .column-header { background: #8b5cf6; color: #ffffff; }
  .col-done .column-header { background: #3fb950; color: #ffffff; }

  /* ---- Cards ---- */
  .card {
    background: #0d1117; border: 1px solid #30363d;
    border-radius: 8px; padding: 10px 12px; margin-bottom: 8px;
    cursor: grab; color: #c9d1d9;
  }
  .card:hover { background: #161b22; }
  .card-selected {
    background: #0c2d6b !important;
    border-color: #58a6ff !important;
    box-shadow: 0 0 0 2px rgba(88,166,255,0.3);
  }
  .card-dragging {
    opacity: 0.35;
    border-style: dashed !important;
    border-color: #58a6ff !important;
    background: #161b22 !important;
  }

  /* Drop placeholder: shown in target column during drag */
  .drop-placeholder {
    display: none; border: 2px dashed #58a6ff;
    border-radius: 8px; padding: 10px 12px; margin-bottom: 8px;
    background: rgba(31,111,235,0.08);
    color: #58a6ff; font-size: 8pt; text-align: center;
    font-weight: 600; min-height: 40px;
  }
  .drop-placeholder-visible { display: block !important; }

  .drop-highlight {
    background: rgba(31,111,235,0.06) !important;
  }

  .card-title { font-weight: 600; font-size: 9pt; margin-bottom: 4px; }
  .card-desc {
    font-size: 8pt; color: #8b949e; display: none;
    margin-top: 6px; padding-top: 6px; border-top: 1px solid #21262d;
  }
  .card-expanded .card-desc { display: block; }
  .card-meta {
    display: flex; justify-content: space-between;
    font-size: 7pt; color: #484f58; margin-top: 6px;
  }
  .card-tag {
    display: inline-block; padding: 1px 6px; border-radius: 4px;
    font-size: 7pt; font-weight: 600;
  }
  .tag-bug { background: #3d1f1f; color: #f85149; }
  .tag-feature { background: #0c2d6b; color: #79c0ff; }
  .tag-chore { background: #1b3a1b; color: #7ee787; }
  .tag-urgent { background: #3d2e0a; color: #d29922; }

  .priority-high { border-left: 3px solid #f85149; }
  .priority-medium { border-left: 3px solid #d29922; }
  .priority-low { border-left: 3px solid #3fb950; }

  /* ---- Buttons ---- */
  .actions {
    padding: 12px 24px; display: flex; gap: 10px; flex-wrap: wrap;
  }
  .btn {
    padding: 7px 14px; border-radius: 8px; font-size: 8pt;
    font-weight: 600; cursor: pointer; border: none;
  }
  .btn-primary { background: #1f6feb; color: #ffffff; }
  .btn-danger { background: #f85149; color: #ffffff; }
  .btn-success { background: #3fb950; color: #ffffff; }
  .btn-warning { background: #d29922; color: #0d1117; }
  .btn-purple { background: #8b5cf6; color: #ffffff; }
  .btn-disabled {
    background: #21262d !important; color: #6e7681 !important;
    cursor: default;
  }

  /* ---- Context menu ---- */
  .ctx-menu {
    display: none; background: #161b22; border: 1px solid #30363d;
    border-radius: 8px; padding: 4px 0; min-width: 180px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.5);
  }
  .ctx-menu-visible { display: block; }
  .ctx-item {
    padding: 7px 16px; font-size: 8pt; color: #c9d1d9; cursor: pointer;
  }
  .ctx-item:hover { background: #21262d; }
  .ctx-item .ctx-shortcut {
    color: #484f58; font-size: 7pt; margin-left: 20px;
  }
  .ctx-sep { height: 1px; background: #21262d; margin: 4px 0; }
  .ctx-danger { color: #f85149; }

  /* ---- Event log ---- */
  .event-log {
    margin: 0 24px 20px 24px; background: #161b22;
    border-radius: 8px; border: 1px solid #21262d;
    padding: 12px 16px; max-height: 140px; overflow: hidden;
  }
  .event-log h3 {
    color: #8b949e; font-size: 8pt; margin: 0 0 8px 0;
    text-transform: uppercase; letter-spacing: 1px;
  }
  .log-entry {
    font-family: monospace; font-size: 8pt; color: #484f58;
    padding: 2px 0;
  }
  .log-entry .log-type { color: #58a6ff; font-weight: bold; }
  .log-entry .log-target { color: #d2a8ff; }
  .log-entry .log-time { color: #30363d; }
  .log-drag { color: #1f6feb !important; }
  .log-drop { color: #3fb950 !important; }

  /* ---- Keyboard hint bar ---- */
  .key-hint {
    display: none; background: #21262d; color: #8b949e;
    font-size: 7pt; padding: 4px 24px;
    border-bottom: 1px solid #30363d;
  }
  .key-hint-visible { display: block !important; }
</style>

<div class="header">
  <h1>Kanban Board</h1>
  <div class="subtitle">Drag cards between columns. Right-click for context menu.</div>
  <div class="keys">
    <span class="key">N</span> new task
    <span class="key">Del</span> delete
    <span class="key">F</span> find bugs
    <span class="key">S</span> shuffle
    <span class="key">E</span> expand/collapse
    <span class="key">1-5</span> move to column
    <span class="key">Tab</span> next card
    <span class="key">Esc</span> deselect
  </div>
</div>

<div class="drag-banner" id="drag-banner">
  <span class="drag-icon">&#x2630;</span>
  Dragging: <span id="drag-title">-</span>
  <span class="drag-target" id="drag-target-col"></span>
</div>

<div class="key-hint" id="key-hint">
  Key pressed: <span class="count" id="key-display">-</span>
</div>

<div class="status-bar">
  <span class="focus-dot" id="focus-dot"></span>
  Tasks: <span class="count" id="total-count">12</span> |
  Selected: <span class="count" id="selected-info">none</span> |
  Mouse: <span class="count" id="mouse-pos">-</span> |
  Last: <span class="count" id="last-action">ready</span>
</div>

<div class="actions">
  <div class="btn btn-primary btn-disabled" id="btn-left">&lt; Move</div>
  <div class="btn btn-primary btn-disabled" id="btn-right">Move &gt;</div>
  <div class="btn btn-danger btn-disabled" id="btn-delete">Delete</div>
  <div class="btn btn-purple" id="btn-search">Find Bugs</div>
  <div class="btn btn-success" id="btn-add">+ New</div>
  <div class="btn btn-warning" id="btn-shuffle">Shuffle</div>
</div>

<div class="board">
  <div class="column col-backlog" id="col-backlog">
    <div class="column-header">Backlog <span class="column-count" id="cnt-backlog">3</span></div>
    <div class="column-body" id="body-backlog">
      <div class="drop-placeholder" id="ph-backlog">Drop here</div>
      <div class="card priority-low" id="card-1">
        <div class="card-title">Update dependencies</div>
        <div class="card-meta"><span class="card-tag tag-chore">chore</span><span>low</span></div>
        <div class="card-desc">Audit and update all npm packages to latest versions.</div>
      </div>
      <div class="card priority-medium" id="card-2">
        <div class="card-title">Dark mode improvements</div>
        <div class="card-meta"><span class="card-tag tag-feature">feature</span><span>med</span></div>
        <div class="card-desc">Fix sidebar colors and code block backgrounds for dark mode.</div>
      </div>
      <div class="card priority-high" id="card-3">
        <div class="card-title">Login page crash</div>
        <div class="card-meta"><span class="card-tag tag-bug">bug</span><span>high</span></div>
        <div class="card-desc">App crashes with special characters in password field.</div>
      </div>
    </div>
  </div>

  <div class="column col-todo" id="col-todo">
    <div class="column-header">To Do <span class="column-count" id="cnt-todo">3</span></div>
    <div class="column-body" id="body-todo">
      <div class="drop-placeholder" id="ph-todo">Drop here</div>
      <div class="card priority-high" id="card-4">
        <div class="card-title">API rate limiting</div>
        <div class="card-meta"><span class="card-tag tag-feature">feature</span><span>high</span></div>
        <div class="card-desc">100 req/min free tier, 1000 for pro.</div>
      </div>
      <div class="card priority-medium" id="card-5">
        <div class="card-title">Fix search indexing</div>
        <div class="card-meta"><span class="card-tag tag-bug">bug</span><span>med</span></div>
        <div class="card-desc">Search results stale after document deletion.</div>
      </div>
      <div class="card priority-low" id="card-6">
        <div class="card-title">Export to CSV</div>
        <div class="card-meta"><span class="card-tag tag-feature">feature</span><span>low</span></div>
        <div class="card-desc">Export data tables as CSV with UTF-8 BOM.</div>
      </div>
    </div>
  </div>

  <div class="column col-progress" id="col-progress">
    <div class="column-header">In Progress <span class="column-count" id="cnt-progress">2</span></div>
    <div class="column-body" id="body-progress">
      <div class="drop-placeholder" id="ph-progress">Drop here</div>
      <div class="card priority-high" id="card-7">
        <div class="card-title">WebSocket reconnect</div>
        <div class="card-meta"><span class="card-tag tag-bug">bug</span><span>high</span></div>
        <div class="card-desc">Implement exponential backoff for WS drops.</div>
      </div>
      <div class="card priority-medium" id="card-8">
        <div class="card-title">Dashboard redesign</div>
        <div class="card-meta"><span class="card-tag tag-feature">feature</span><span>med</span></div>
        <div class="card-desc">Responsive grid layout with charts.</div>
      </div>
    </div>
  </div>

  <div class="column col-review" id="col-review">
    <div class="column-header">Review <span class="column-count" id="cnt-review">2</span></div>
    <div class="column-body" id="body-review">
      <div class="drop-placeholder" id="ph-review">Drop here</div>
      <div class="card priority-medium" id="card-9">
        <div class="card-title">OAuth2 integration</div>
        <div class="card-meta"><span class="card-tag tag-feature">feature</span><span>med</span></div>
        <div class="card-desc">Google and GitHub OAuth2 with token refresh.</div>
      </div>
      <div class="card priority-high" id="card-10">
        <div class="card-title">Memory leak fix</div>
        <div class="card-meta"><span class="card-tag tag-bug">bug</span><span>high</span></div>
        <div class="card-desc">Canvas renderer leaks ~2MB/hour.</div>
      </div>
    </div>
  </div>

  <div class="column col-done" id="col-done">
    <div class="column-header">Done <span class="column-count" id="cnt-done">2</span></div>
    <div class="column-body" id="body-done">
      <div class="drop-placeholder" id="ph-done">Drop here</div>
      <div class="card priority-medium" id="card-11">
        <div class="card-title">Pagination component</div>
        <div class="card-meta"><span class="card-tag tag-feature">feature</span><span>med</span></div>
        <div class="card-desc">Reusable pagination with keyboard navigation.</div>
      </div>
      <div class="card priority-high" id="card-12">
        <div class="card-title">Timezone bug fix</div>
        <div class="card-meta"><span class="card-tag tag-bug">bug</span><span>high</span></div>
        <div class="card-desc">Now uses Intl API for user's local timezone.</div>
      </div>
    </div>
  </div>
</div>

<div class="ctx-menu" id="ctx-menu">
  <div class="ctx-item" id="ctx-expand">Expand / Collapse <span class="ctx-shortcut">E</span></div>
  <div class="ctx-item" id="ctx-select">Select <span class="ctx-shortcut">Click</span></div>
  <div class="ctx-sep"></div>
  <div class="ctx-item" id="ctx-move-right">Move Right <span class="ctx-shortcut">&gt;</span></div>
  <div class="ctx-item" id="ctx-move-left">Move Left <span class="ctx-shortcut">&lt;</span></div>
  <div class="ctx-sep"></div>
  <div class="ctx-item" id="ctx-prio-high">Priority: High</div>
  <div class="ctx-item" id="ctx-prio-med">Priority: Medium</div>
  <div class="ctx-item" id="ctx-prio-low">Priority: Low</div>
  <div class="ctx-sep"></div>
  <div class="ctx-item ctx-danger" id="ctx-delete">Delete <span class="ctx-shortcut">Del</span></div>
</div>

<div class="event-log" id="event-log">
  <h3>Event Log</h3>
  <div class="log-entry" id="log-5"><span class="log-time">[--:--:--]</span> <span class="log-type">READY</span> Board loaded. Drag cards or use keyboard shortcuts.</div>
  <div class="log-entry" id="log-4"><span class="log-time">[--:--:--]</span> <span class="log-type">HINT</span> Right-click a card for context menu</div>
  <div class="log-entry" id="log-3"><span class="log-time">[--:--:--]</span> <span class="log-type">HINT</span> Press <span class="log-target">Tab</span> to cycle cards, <span class="log-target">Esc</span> to deselect</div>
  <div class="log-entry" id="log-2"><span class="log-time">[--:--:--]</span> <span class="log-type">HINT</span> Press <span class="log-target">N</span> to add a new task</div>
  <div class="log-entry" id="log-1"><span class="log-time">[--:--:--]</span> <span class="log-type">HINT</span> <span class="log-target">F</span> to find bugs, <span class="log-target">S</span> to shuffle</div>
</div>

<div class="drag-ghost" id="drag-ghost">
  <span id="ghost-title">Card</span>
  <div class="ghost-tag" id="ghost-meta">feature</div>
</div>
)");

// Column display names for the drag banner
static const char *COLUMN_DISPLAY[] = {"Backlog", "To Do", "In Progress", "Review", "Done"};

class EventsDemoFrame : public wxFrame {
public:
  EventsDemoFrame()
      : wxFrame(nullptr, wxID_ANY, "Kanban Board - Events Demo",
                wxDefaultPosition, wxSize(1200, 800)) {
    std::srand(std::time(nullptr));

    m_html = new wxHtmlEditWidget(this);
    m_html->SetReadOnly(true);
    m_html->SetHTML(KANBAN_HTML);

    auto *sizer = new wxBoxSizer(wxVERTICAL);
    sizer->Add(m_html, 1, wxEXPAND);
    SetSizer(sizer);

    SetupEventListeners();
    m_logCounter = 6;
    m_nextCardId = 13;
  }

private:
  wxHtmlEditWidget *m_html;
  wxString m_selectedCardId;
  wxString m_ctxCardId;
  wxString m_dragCardId;
  wxString m_dragCardTitle;
  wxString m_currentDropCol;     // column currently being dragged over
  int m_logCounter;
  int m_nextCardId;

  const std::vector<wxString> m_columns = {
    "backlog", "todo", "progress", "review", "done"
  };

  Box *FindAncestorWithClass(Box *box, const wxString &cls) {
    while (box) {
      if (m_html->HasClass(box, cls)) return box;
      box = m_html->GetElementParent(box);
    }
    return nullptr;
  }

  wxString GetCardTitle(Box *card) {
    for (Box *child : m_html->GetElementChildren(card))
      if (m_html->HasClass(child, "card-title"))
        return m_html->GetTextContent(child);
    return "untitled";
  }

  wxString FindCardColumn(const wxString &cardId) {
    Box *card = m_html->QuerySelector("#" + cardId);
    if (!card) return "";
    Box *col = FindAncestorWithClass(card, "column");
    if (!col) return "";
    wxString colId = m_html->GetAttribute(col, "id");
    return colId.StartsWith("col-") ? colId.Mid(4) : colId;
  }

  int ColumnIndex(const wxString &colName) {
    for (int i = 0; i < (int)m_columns.size(); i++)
      if (m_columns[i] == colName) return i;
    return -1;
  }

  wxString ColumnDisplayName(const wxString &colName) {
    int idx = ColumnIndex(colName);
    if (idx >= 0 && idx < 5) return COLUMN_DISPLAY[idx];
    return colName;
  }

  wxString GetColumnForBody(Box *body) {
    Box *col = FindAncestorWithClass(body, "column");
    if (!col) return "";
    wxString colId = m_html->GetAttribute(col, "id");
    return colId.StartsWith("col-") ? colId.Mid(4) : colId;
  }

  void SetupEventListeners() {
    // ==================== CLICK: select card ====================
    m_html->AddEventListener(".card", HtmlEventType::Click,
      [this](HtmlEvent &evt) {
        Box *card = FindAncestorWithClass(evt.target, "card");
        if (!card) return;
        SelectCard(m_html->GetAttribute(card, "id"));
        evt.StopPropagation();
      });

    // ==================== DOUBLE CLICK: expand/collapse ====================
    m_html->AddEventListener(".card", HtmlEventType::DblClick,
      [this](HtmlEvent &evt) {
        Box *card = FindAncestorWithClass(evt.target, "card");
        if (!card) return;
        ToggleExpand(card);
        evt.StopPropagation();
      });

    // ==================== MOUSE ENTER/LEAVE: hover effect ====================
    m_html->AddEventListener(".card", HtmlEventType::MouseEnter,
      [this](HtmlEvent &evt) {
        Box *card = FindAncestorWithClass(evt.target, "card");
        if (!card) return;
        wxString id = m_html->GetAttribute(card, "id");
        if (id != m_selectedCardId && id != m_dragCardId)
          m_html->SetStyleProperty(card, "border-color", "#484f58");
      });

    m_html->AddEventListener(".card", HtmlEventType::MouseLeave,
      [this](HtmlEvent &evt) {
        Box *card = FindAncestorWithClass(evt.target, "card");
        if (!card) return;
        wxString id = m_html->GetAttribute(card, "id");
        if (id != m_selectedCardId && id != m_dragCardId)
          m_html->SetStyleProperty(card, "border-color", "#30363d");
      });

    // ==================== MOUSE MOVE: coords in status bar ====================
    m_html->AddEventListener("*", HtmlEventType::MouseMove,
      [this](HtmlEvent &evt) {
        Box *pos = m_html->QuerySelector("#mouse-pos");
        if (pos)
          m_html->SetTextContent(pos, wxString::Format("(%d,%d)",
            evt.docPos.x, evt.docPos.y));
      });

    // ==================== DRAG START ====================
    m_html->AddEventListener(".card", HtmlEventType::DragStart,
      [this](HtmlEvent &evt) {
        Box *card = FindAncestorWithClass(evt.target, "card");
        if (!card) return;
        m_dragCardId = m_html->GetAttribute(card, "id");
        m_dragCardTitle = GetCardTitle(card);
        m_currentDropCol.clear();

        m_html->BeginBatchUpdate();

        // Ghost the source card
        m_html->AddClass(card, "card-dragging");

        // Show drag banner
        Box *banner = m_html->QuerySelector("#drag-banner");
        if (banner) m_html->AddClass(banner, "drag-banner-visible");
        Box *dragTitle = m_html->QuerySelector("#drag-title");
        if (dragTitle) m_html->SetTextContent(dragTitle, m_dragCardTitle);
        Box *dragTargetCol = m_html->QuerySelector("#drag-target-col");
        if (dragTargetCol) m_html->SetTextContent(dragTargetCol, "");

        // Show drag ghost at cursor position
        Box *ghost = m_html->QuerySelector("#drag-ghost");
        if (ghost) {
          // Populate ghost with card info
          Box *ghostTitle = m_html->QuerySelector("#ghost-title");
          if (ghostTitle) m_html->SetTextContent(ghostTitle, m_dragCardTitle);
          // Get the card's tag text
          Box *ghostMeta = m_html->QuerySelector("#ghost-meta");
          if (ghostMeta) {
            wxString tagText;
            auto cardChildren = m_html->GetElementChildren(card);
            for (Box *ch : cardChildren) {
              if (m_html->HasClass(ch, "card-meta")) {
                auto metaKids = m_html->GetElementChildren(ch);
                if (!metaKids.empty())
                  tagText = m_html->GetTextContent(metaKids[0]);
                break;
              }
            }
            m_html->SetTextContent(ghostMeta, tagText);
          }
          m_html->SetStyleProperty(ghost, "left",
            wxString::Format("%dpx", evt.docPos.x + 12));
          m_html->SetStyleProperty(ghost, "top",
            wxString::Format("%dpx", evt.docPos.y + 12));
          m_html->AddClass(ghost, "drag-ghost-visible");
        }

        m_html->EndBatchUpdate();
        AddLogEntry("DRAG", "Started: " + m_dragCardTitle);
      });

    // ==================== DRAG (continuous): move ghost ====================
    m_html->AddEventListener("*", HtmlEventType::Drag,
      [this](HtmlEvent &evt) {
        if (m_dragCardId.empty()) return;
        // Move drag ghost to follow cursor
        Box *ghost = m_html->QuerySelector("#drag-ghost");
        if (ghost) {
          m_html->SetStyleProperty(ghost, "left",
            wxString::Format("%dpx", evt.docPos.x + 12));
          m_html->SetStyleProperty(ghost, "top",
            wxString::Format("%dpx", evt.docPos.y + 12));
        }
        // Update mouse position
        Box *pos = m_html->QuerySelector("#mouse-pos");
        if (pos)
          m_html->SetTextContent(pos, wxString::Format("(%d,%d)",
            evt.docPos.x, evt.docPos.y));
      });

    // ==================== DRAG OVER: update banner with target column ====================
    m_html->AddEventListener(".column-body", HtmlEventType::DragOver,
      [this](HtmlEvent &evt) {
        if (m_dragCardId.empty()) return;
        Box *body = FindAncestorWithClass(evt.target, "column-body");
        if (!body) return;
        wxString col = GetColumnForBody(body);
        if (!col.empty() && col != m_currentDropCol) {
          m_currentDropCol = col;
          Box *dragTargetCol = m_html->QuerySelector("#drag-target-col");
          if (dragTargetCol)
            m_html->SetTextContent(dragTargetCol, "-> " + ColumnDisplayName(col));
        }
      });

    // ==================== DRAG ENTER: highlight drop target ====================
    m_html->AddEventListener(".column-body", HtmlEventType::DragEnter,
      [this](HtmlEvent &evt) {
        if (m_dragCardId.empty()) return;
        Box *body = FindAncestorWithClass(evt.target, "column-body");
        if (!body) return;

        wxString col = GetColumnForBody(body);
        wxString sourceCol = FindCardColumn(m_dragCardId);

        m_html->BeginBatchUpdate();

        // Highlight column body
        m_html->AddClass(body, "drop-highlight");

        // Glow on column header
        Box *colBox = FindAncestorWithClass(body, "column");
        if (colBox) {
          m_html->AddClass(colBox, "column-drop-active");
          for (Box *ch : m_html->GetElementChildren(colBox))
            if (m_html->HasClass(ch, "column-header"))
              m_html->AddClass(ch, "column-header-glow");
        }

        // Show drop placeholder (only in different column)
        if (col != sourceCol) {
          Box *ph = m_html->QuerySelector("#ph-" + col);
          if (ph) {
            m_html->SetTextContent(ph, "Drop: " + m_dragCardTitle);
            m_html->AddClass(ph, "drop-placeholder-visible");
          }
        }

        m_html->EndBatchUpdate();
      });

    // ==================== DRAG LEAVE: remove highlight ====================
    m_html->AddEventListener(".column-body", HtmlEventType::DragLeave,
      [this](HtmlEvent &evt) {
        if (m_dragCardId.empty()) return;
        Box *body = FindAncestorWithClass(evt.target, "column-body");
        if (!body) return;

        wxString col = GetColumnForBody(body);

        m_html->BeginBatchUpdate();

        m_html->RemoveClass(body, "drop-highlight");

        Box *colBox = FindAncestorWithClass(body, "column");
        if (colBox) {
          m_html->RemoveClass(colBox, "column-drop-active");
          for (Box *ch : m_html->GetElementChildren(colBox))
            if (m_html->HasClass(ch, "column-header"))
              m_html->RemoveClass(ch, "column-header-glow");
        }

        // Hide placeholder
        Box *ph = m_html->QuerySelector("#ph-" + col);
        if (ph) m_html->RemoveClass(ph, "drop-placeholder-visible");

        m_html->EndBatchUpdate();
      });

    // ==================== DROP: move card to column ====================
    m_html->AddEventListener(".column-body", HtmlEventType::Drop,
      [this](HtmlEvent &evt) {
        if (m_dragCardId.empty()) return;
        Box *targetBody = FindAncestorWithClass(evt.target, "column-body");
        if (!targetBody) return;
        DropCard(m_dragCardId, targetBody);
        evt.StopPropagation();
      });

    // Also allow dropping on column headers
    m_html->AddEventListener(".column-header", HtmlEventType::Drop,
      [this](HtmlEvent &evt) {
        if (m_dragCardId.empty()) return;
        Box *col = FindAncestorWithClass(evt.target, "column");
        if (!col) return;
        for (Box *child : m_html->GetElementChildren(col))
          if (m_html->HasClass(child, "column-body")) {
            DropCard(m_dragCardId, child);
            break;
          }
        evt.StopPropagation();
      });

    // Also allow dropping on the placeholder itself
    m_html->AddEventListener(".drop-placeholder", HtmlEventType::Drop,
      [this](HtmlEvent &evt) {
        if (m_dragCardId.empty()) return;
        Box *body = FindAncestorWithClass(evt.target, "column-body");
        if (!body) return;
        DropCard(m_dragCardId, body);
        evt.StopPropagation();
      });

    // ==================== DRAG END: cleanup ====================
    m_html->AddEventListener(".card", HtmlEventType::DragEnd,
      [this](HtmlEvent &) {
        CleanupDragState();
      });

    // ==================== CONTEXT MENU (right-click) ====================
    m_html->AddEventListener(".card", HtmlEventType::ContextMenu,
      [this](HtmlEvent &evt) {
        Box *card = FindAncestorWithClass(evt.target, "card");
        if (!card) return;
        m_ctxCardId = m_html->GetAttribute(card, "id");
        ShowContextMenu(evt.docPos);
        AddLogEntry("CONTEXT", "Right-click: " + GetCardTitle(card));
        evt.StopPropagation();
      });

    // Context menu item clicks
    m_html->AddEventListener("#ctx-expand", HtmlEventType::Click,
      [this](HtmlEvent &evt) { CtxAction("expand"); evt.StopPropagation(); });
    m_html->AddEventListener("#ctx-select", HtmlEventType::Click,
      [this](HtmlEvent &evt) { CtxAction("select"); evt.StopPropagation(); });
    m_html->AddEventListener("#ctx-move-right", HtmlEventType::Click,
      [this](HtmlEvent &evt) { CtxAction("move-right"); evt.StopPropagation(); });
    m_html->AddEventListener("#ctx-move-left", HtmlEventType::Click,
      [this](HtmlEvent &evt) { CtxAction("move-left"); evt.StopPropagation(); });
    m_html->AddEventListener("#ctx-delete", HtmlEventType::Click,
      [this](HtmlEvent &evt) { CtxAction("delete"); evt.StopPropagation(); });
    m_html->AddEventListener("#ctx-prio-high", HtmlEventType::Click,
      [this](HtmlEvent &evt) { CtxAction("prio-high"); evt.StopPropagation(); });
    m_html->AddEventListener("#ctx-prio-med", HtmlEventType::Click,
      [this](HtmlEvent &evt) { CtxAction("prio-med"); evt.StopPropagation(); });
    m_html->AddEventListener("#ctx-prio-low", HtmlEventType::Click,
      [this](HtmlEvent &evt) { CtxAction("prio-low"); evt.StopPropagation(); });

    // Click anywhere hides context menu
    m_html->AddEventListener("*", HtmlEventType::Click,
      [this](HtmlEvent &) { HideContextMenu(); });

    // ==================== KEYBOARD ====================
    m_html->AddEventListener("*", HtmlEventType::KeyDown,
      [this](HtmlEvent &evt) {
        if (evt.ctrlKey || evt.metaKey) return;
        switch (evt.keyCode) {
          case 'N': AddNewTask(); evt.PreventDefault(); break;
          case WXK_DELETE:
          case WXK_BACK:
            if (!m_selectedCardId.empty()) {
              DeleteCard(m_selectedCardId);
              evt.PreventDefault();
            }
            break;
          case 'F': FindAndHighlightBugs(); evt.PreventDefault(); break;
          case 'S': ShuffleCards(); evt.PreventDefault(); break;
          case 'E':
            if (!m_selectedCardId.empty()) {
              Box *c = m_html->QuerySelector("#" + m_selectedCardId);
              if (c) ToggleExpand(c);
              evt.PreventDefault();
            }
            break;
          case '1': case '2': case '3': case '4': case '5':
            if (!m_selectedCardId.empty()) {
              MoveCardToColumn(m_selectedCardId, evt.keyCode - '1');
              evt.PreventDefault();
            }
            break;
          case WXK_TAB:
            CycleSelection(evt.shiftKey ? -1 : 1);
            evt.PreventDefault();
            break;
          case WXK_ESCAPE:
            if (!m_selectedCardId.empty()) {
              DeselectAll();
              evt.PreventDefault();
            }
            break;
          default: break;
        }
      });

    // KeyUp: show key hint briefly
    m_html->AddEventListener("*", HtmlEventType::KeyUp,
      [this](HtmlEvent &evt) {
        Box *hint = m_html->QuerySelector("#key-hint");
        Box *keyDisp = m_html->QuerySelector("#key-display");
        if (hint && keyDisp) {
          wxString keyName;
          if (evt.keyCode >= 32 && evt.keyCode < 127)
            keyName = wxString::Format("'%c' (%d)", (char)evt.keyCode, evt.keyCode);
          else
            keyName = wxString::Format("keyCode=%d", evt.keyCode);
          if (evt.shiftKey) keyName = "Shift+" + keyName;
          if (evt.ctrlKey) keyName = "Ctrl+" + keyName;
          if (evt.altKey) keyName = "Alt+" + keyName;
          m_html->SetTextContent(keyDisp, keyName);
          m_html->AddClass(hint, "key-hint-visible");
        }
      });

    // ==================== FOCUS / BLUR ====================
    m_html->AddEventListener("*", HtmlEventType::Focus,
      [this](HtmlEvent &) {
        Box *dot = m_html->QuerySelector("#focus-dot");
        if (dot) m_html->AddClass(dot, "focus-dot-active");
        AddLogEntry("FOCUS", "Widget focused");
      });

    m_html->AddEventListener("*", HtmlEventType::Blur,
      [this](HtmlEvent &) {
        Box *dot = m_html->QuerySelector("#focus-dot");
        if (dot) m_html->RemoveClass(dot, "focus-dot-active");
        // Hide key hint when losing focus
        Box *hint = m_html->QuerySelector("#key-hint");
        if (hint) m_html->RemoveClass(hint, "key-hint-visible");
        AddLogEntry("BLUR", "Widget lost focus");
      });

    // ==================== SCROLL ====================
    m_html->AddEventListener("*", HtmlEventType::Scroll,
      [this](HtmlEvent &) {
        static int scrollCount = 0;
        if (++scrollCount % 10 == 0)
          AddLogEntry("SCROLL", wxString::Format("Scrolled (%d events)", scrollCount));
      });

    // ==================== BUTTON CLICKS ====================
    m_html->AddEventListener("#btn-right", HtmlEventType::Click,
      [this](HtmlEvent &evt) {
        if (!m_selectedCardId.empty()) MoveCard(m_selectedCardId, 1);
        evt.StopPropagation();
      });
    m_html->AddEventListener("#btn-left", HtmlEventType::Click,
      [this](HtmlEvent &evt) {
        if (!m_selectedCardId.empty()) MoveCard(m_selectedCardId, -1);
        evt.StopPropagation();
      });
    m_html->AddEventListener("#btn-delete", HtmlEventType::Click,
      [this](HtmlEvent &evt) {
        if (!m_selectedCardId.empty()) DeleteCard(m_selectedCardId);
        evt.StopPropagation();
      });
    m_html->AddEventListener("#btn-search", HtmlEventType::Click,
      [this](HtmlEvent &evt) { FindAndHighlightBugs(); evt.StopPropagation(); });
    m_html->AddEventListener("#btn-add", HtmlEventType::Click,
      [this](HtmlEvent &evt) { AddNewTask(); evt.StopPropagation(); });
    m_html->AddEventListener("#btn-shuffle", HtmlEventType::Click,
      [this](HtmlEvent &evt) { ShuffleCards(); evt.StopPropagation(); });

    // ==================== COLUMN HEADER: collapse/expand ====================
    m_html->AddEventListener(".column-header", HtmlEventType::Click,
      [this](HtmlEvent &evt) {
        Box *col = FindAncestorWithClass(evt.target, "column");
        if (!col) return;
        for (Box *child : m_html->GetElementChildren(col)) {
          if (m_html->HasClass(child, "column-body")) {
            if (m_html->IsVisible(child)) {
              m_html->Hide(child);
              AddLogEntry("COLLAPSE", "Column collapsed");
            } else {
              m_html->Show(child);
              AddLogEntry("EXPAND", "Column expanded");
            }
            break;
          }
        }
        evt.StopPropagation();
      });

    // ==================== COLUMN HEADER: drag enter/leave glow ====================
    m_html->AddEventListener(".column-header", HtmlEventType::DragEnter,
      [this](HtmlEvent &evt) {
        if (m_dragCardId.empty()) return;
        Box *hdr = FindAncestorWithClass(evt.target, "column-header");
        if (hdr) m_html->AddClass(hdr, "column-header-glow");
      });
    m_html->AddEventListener(".column-header", HtmlEventType::DragLeave,
      [this](HtmlEvent &evt) {
        Box *hdr = FindAncestorWithClass(evt.target, "column-header");
        if (hdr) m_html->RemoveClass(hdr, "column-header-glow");
      });
  }

  // ==================== Actions ====================

  void SelectCard(const wxString &cardId) {
    m_html->BeginBatchUpdate();

    // Deselect previous
    if (!m_selectedCardId.empty()) {
      Box *prev = m_html->QuerySelector("#" + m_selectedCardId);
      if (prev) {
        m_html->RemoveClass(prev, "card-selected");
        m_html->SetStyleProperty(prev, "border-color", "#30363d");
      }
    }

    if (m_selectedCardId == cardId) {
      m_selectedCardId.clear();
      UpdateUI();
      AddLogEntry("SELECT", "Deselected");
      m_html->EndBatchUpdate();
      return;
    }

    m_selectedCardId = cardId;
    Box *card = m_html->QuerySelector("#" + cardId);
    if (card) {
      m_html->AddClass(card, "card-selected");
      AddLogEntry("SELECT", GetCardTitle(card));
    }

    UpdateUI();
    m_html->EndBatchUpdate();
  }

  void DeselectAll() {
    if (!m_selectedCardId.empty()) {
      Box *prev = m_html->QuerySelector("#" + m_selectedCardId);
      if (prev) {
        m_html->RemoveClass(prev, "card-selected");
        m_html->SetStyleProperty(prev, "border-color", "#30363d");
      }
      m_selectedCardId.clear();
      UpdateUI();
      AddLogEntry("SELECT", "Deselected all");
    }
  }

  void CycleSelection(int direction) {
    auto allCards = m_html->QuerySelectorAll(".card");
    if (allCards.empty()) return;

    if (m_selectedCardId.empty()) {
      // Select first card
      wxString id = m_html->GetAttribute(allCards[0], "id");
      SelectCard(id);
      return;
    }

    // Find current index
    int curIdx = -1;
    for (int i = 0; i < (int)allCards.size(); i++) {
      if (m_html->GetAttribute(allCards[i], "id") == m_selectedCardId) {
        curIdx = i;
        break;
      }
    }

    int nextIdx = (curIdx + direction + (int)allCards.size()) % (int)allCards.size();
    wxString nextId = m_html->GetAttribute(allCards[nextIdx], "id");

    // Deselect current first
    Box *prev = m_html->QuerySelector("#" + m_selectedCardId);
    if (prev) {
      m_html->RemoveClass(prev, "card-selected");
      m_html->SetStyleProperty(prev, "border-color", "#30363d");
    }
    m_selectedCardId.clear();

    SelectCard(nextId);
    Box *next = m_html->QuerySelector("#" + nextId);
    if (next) m_html->ScrollIntoView(next);
  }

  void ToggleExpand(Box *card) {
    if (m_html->HasClass(card, "card-expanded")) {
      m_html->RemoveClass(card, "card-expanded");
      AddLogEntry("TOGGLE", "Collapsed " + GetCardTitle(card));
    } else {
      m_html->AddClass(card, "card-expanded");
      AddLogEntry("TOGGLE", "Expanded " + GetCardTitle(card));
    }
  }

  void CleanupDragState() {
    m_html->BeginBatchUpdate();

    // Remove dragging class from source card
    if (!m_dragCardId.empty()) {
      Box *card = m_html->QuerySelector("#" + m_dragCardId);
      if (card) m_html->RemoveClass(card, "card-dragging");
    }

    // Hide drag banner and ghost
    Box *banner = m_html->QuerySelector("#drag-banner");
    if (banner) m_html->RemoveClass(banner, "drag-banner-visible");
    Box *ghost = m_html->QuerySelector("#drag-ghost");
    if (ghost) m_html->RemoveClass(ghost, "drag-ghost-visible");

    // Remove all drop highlights, column glows, and placeholders
    for (auto &col : m_columns) {
      Box *body = m_html->QuerySelector("#body-" + col);
      if (body) m_html->RemoveClass(body, "drop-highlight");

      Box *colBox = m_html->QuerySelector("#col-" + col);
      if (colBox) {
        m_html->RemoveClass(colBox, "column-drop-active");
        for (Box *ch : m_html->GetElementChildren(colBox))
          if (m_html->HasClass(ch, "column-header"))
            m_html->RemoveClass(ch, "column-header-glow");
      }

      Box *ph = m_html->QuerySelector("#ph-" + col);
      if (ph) m_html->RemoveClass(ph, "drop-placeholder-visible");
    }

    m_dragCardId.clear();
    m_dragCardTitle.clear();
    m_currentDropCol.clear();

    m_html->EndBatchUpdate();
  }

  void DropCard(const wxString &cardId, Box *targetBody) {
    Box *card = m_html->QuerySelector("#" + cardId);
    if (!card) return;

    wxString title = GetCardTitle(card);
    wxString cardHTML = m_html->GetOuterHTML(card);

    // Remove dragging class before serializing
    m_html->RemoveClass(card, "card-dragging");
    cardHTML = m_html->GetOuterHTML(card);

    m_html->BeginBatchUpdate();
    m_html->RemoveChild(card);
    m_html->SetInnerHTML(targetBody, m_html->GetInnerHTML(targetBody) + cardHTML);

    // Cleanup all drag visuals
    CleanupDragState();

    Box *col = FindAncestorWithClass(targetBody, "column");
    wxString colName = "?";
    if (col) {
      wxString colId = m_html->GetAttribute(col, "id");
      if (colId.StartsWith("col-")) colName = colId.Mid(4);
    }

    UpdateColumnCounts();
    AddLogEntry("DROP", title + " -> " + ColumnDisplayName(colName));

    // Re-select if needed
    if (m_selectedCardId == cardId) {
      Box *moved = m_html->QuerySelector("#" + cardId);
      if (moved) {
        m_html->AddClass(moved, "card-selected");
      }
      UpdateUI();
    }

    m_html->EndBatchUpdate();
  }

  void MoveCard(const wxString &cardId, int direction) {
    wxString colName = FindCardColumn(cardId);
    int idx = ColumnIndex(colName);
    if (idx < 0) return;
    int newIdx = idx + direction;
    if (newIdx < 0 || newIdx >= (int)m_columns.size()) return;
    MoveCardToColumn(cardId, newIdx);
  }

  void MoveCardToColumn(const wxString &cardId, int colIdx) {
    if (colIdx < 0 || colIdx >= (int)m_columns.size()) return;
    Box *card = m_html->QuerySelector("#" + cardId);
    if (!card) return;

    wxString curCol = FindCardColumn(cardId);
    if (ColumnIndex(curCol) == colIdx) return;

    wxString title = GetCardTitle(card);
    wxString cardHTML = m_html->GetOuterHTML(card);

    m_html->BeginBatchUpdate();
    m_html->RemoveChild(card);

    Box *newBody = m_html->QuerySelector("#body-" + m_columns[colIdx]);
    if (newBody)
      m_html->SetInnerHTML(newBody, m_html->GetInnerHTML(newBody) + cardHTML);

    if (m_selectedCardId == cardId) {
      Box *moved = m_html->QuerySelector("#" + cardId);
      if (moved) {
        m_html->AddClass(moved, "card-selected");
        m_html->ScrollIntoView(moved);
      }
    }

    UpdateColumnCounts();
    UpdateUI();
    AddLogEntry("MOVE", title + " -> " + ColumnDisplayName(m_columns[colIdx]));
    m_html->EndBatchUpdate();
  }

  void DeleteCard(const wxString &cardId) {
    Box *card = m_html->QuerySelector("#" + cardId);
    if (!card) return;
    wxString title = GetCardTitle(card);

    m_html->BeginBatchUpdate();
    m_html->RemoveChild(card);
    if (m_selectedCardId == cardId) m_selectedCardId.clear();
    UpdateColumnCounts();
    UpdateUI();
    AddLogEntry("DELETE", title);
    m_html->EndBatchUpdate();
  }

  void ChangePriority(const wxString &cardId, const wxString &prio) {
    Box *card = m_html->QuerySelector("#" + cardId);
    if (!card) return;

    m_html->RemoveClass(card, "priority-high");
    m_html->RemoveClass(card, "priority-medium");
    m_html->RemoveClass(card, "priority-low");
    m_html->AddClass(card, "priority-" + prio);

    // Update the priority text in meta
    auto children = m_html->GetElementChildren(card);
    for (Box *ch : children) {
      if (m_html->HasClass(ch, "card-meta")) {
        auto metaChildren = m_html->GetElementChildren(ch);
        if (metaChildren.size() >= 2) {
          wxString label = prio == "high" ? "high" : prio == "medium" ? "med" : "low";
          m_html->SetTextContent(metaChildren[1], label);
        }
        break;
      }
    }

    AddLogEntry("PRIORITY", GetCardTitle(card) + " -> " + prio);
  }

  void ShowContextMenu(const wxPoint &pos) {
    Box *menu = m_html->QuerySelector("#ctx-menu");
    if (!menu) return;
    m_html->AddClass(menu, "ctx-menu-visible");
    m_html->SetStyleProperty(menu, "position", "absolute");
    m_html->SetStyleProperty(menu, "left", wxString::Format("%dpx", pos.x));
    m_html->SetStyleProperty(menu, "top", wxString::Format("%dpx", pos.y));
  }

  void HideContextMenu() {
    Box *menu = m_html->QuerySelector("#ctx-menu");
    if (menu) m_html->RemoveClass(menu, "ctx-menu-visible");
  }

  void CtxAction(const wxString &action) {
    HideContextMenu();
    if (m_ctxCardId.empty()) return;

    if (action == "expand") {
      Box *card = m_html->QuerySelector("#" + m_ctxCardId);
      if (card) ToggleExpand(card);
    } else if (action == "select") {
      SelectCard(m_ctxCardId);
    } else if (action == "move-right") {
      MoveCard(m_ctxCardId, 1);
    } else if (action == "move-left") {
      MoveCard(m_ctxCardId, -1);
    } else if (action == "delete") {
      DeleteCard(m_ctxCardId);
    } else if (action == "prio-high") {
      ChangePriority(m_ctxCardId, "high");
    } else if (action == "prio-med") {
      ChangePriority(m_ctxCardId, "medium");
    } else if (action == "prio-low") {
      ChangePriority(m_ctxCardId, "low");
    }
    m_ctxCardId.clear();
  }

  void FindAndHighlightBugs() {
    auto bugTags = m_html->FindElementsByText("bug");
    int count = 0;

    m_html->BeginBatchUpdate();
    auto allCards = m_html->QuerySelectorAll(".card");
    for (Box *c : allCards)
      m_html->SetStyleProperty(c, "border-color", "#30363d");

    for (Box *tag : bugTags) {
      if (!m_html->HasClass(tag, "tag-bug")) continue;
      Box *card = FindAncestorWithClass(tag, "card");
      if (!card) continue;
      m_html->SetStyleProperty(card, "border-color", "#f85149");
      m_html->AddClass(card, "card-expanded");
      count++;
    }

    if (count > 0) {
      for (Box *tag : bugTags) {
        if (!m_html->HasClass(tag, "tag-bug")) continue;
        Box *card = FindAncestorWithClass(tag, "card");
        if (card) {
          m_html->ScrollIntoView(card);
          wxRect r = m_html->GetBoundingRect(card);
          AddLogEntry("SEARCH",
            wxString::Format("Found %d bugs, first at (%d,%d)", count, r.x, r.y));
          break;
        }
      }
    } else {
      AddLogEntry("SEARCH", "No bugs found");
    }
    m_html->EndBatchUpdate();
  }

  void AddNewTask() {
    static const char *titles[] = {
      "Fix scroll jitter", "Add dark mode toggle", "Refactor auth module",
      "Update API docs", "Cache invalidation bug", "Add unit tests",
      "Mobile responsive layout", "Fix race condition", "Add notifications",
      "Optimize bundle size", "Database migration", "Add logging",
    };
    static const char *tags[] = { "bug", "feature", "chore", "urgent" };
    static const char *priorities[] = { "low", "medium", "high" };

    wxString cardId = wxString::Format("card-%d", m_nextCardId++);
    wxString tag = tags[std::rand() % 4];
    wxString prio = priorities[std::rand() % 3];
    wxString title = titles[std::rand() % 12];

    wxString cardHTML = wxString::Format(
      "<div class=\"card priority-%s\" id=\"%s\">"
      "<div class=\"card-title\">%s</div>"
      "<div class=\"card-meta\"><span class=\"card-tag tag-%s\">%s</span>"
      "<span>%s</span></div>"
      "<div class=\"card-desc\">Auto-generated task.</div></div>",
      prio, cardId, title, tag, tag, prio);

    m_html->BeginBatchUpdate();
    Box *backlog = m_html->QuerySelector("#body-backlog");
    if (backlog)
      m_html->SetInnerHTML(backlog, m_html->GetInnerHTML(backlog) + cardHTML);
    UpdateColumnCounts();
    AddLogEntry("ADD", wxString::Format("%s [%s/%s]", title, tag, prio));
    Box *newCard = m_html->QuerySelector("#" + cardId);
    if (newCard) m_html->ScrollIntoView(newCard);
    m_html->EndBatchUpdate();
  }

  void ShuffleCards() {
    auto allCards = m_html->QuerySelectorAll(".card");
    if (allCards.empty()) return;

    m_html->BeginBatchUpdate();
    std::vector<wxString> htmls;
    for (Box *card : allCards) {
      m_html->RemoveClass(card, "card-selected");
      m_html->SetStyleProperty(card, "border-color", "#30363d");
      htmls.push_back(m_html->GetOuterHTML(card));
    }

    for (int i = (int)htmls.size() - 1; i > 0; i--)
      std::swap(htmls[i], htmls[std::rand() % (i + 1)]);

    // Keep placeholders, clear cards
    for (auto &col : m_columns) {
      Box *body = m_html->QuerySelector("#body-" + col);
      if (body) {
        wxString phHTML = wxString::Format(
          "<div class=\"drop-placeholder\" id=\"ph-%s\">Drop here</div>", col);
        m_html->SetInnerHTML(body, phHTML);
      }
    }

    for (int i = 0; i < (int)htmls.size(); i++) {
      int ci = i % (int)m_columns.size();
      Box *body = m_html->QuerySelector("#body-" + m_columns[ci]);
      if (body)
        m_html->SetInnerHTML(body, m_html->GetInnerHTML(body) + htmls[i]);
    }

    m_selectedCardId.clear();
    UpdateColumnCounts();
    UpdateUI();
    AddLogEntry("SHUFFLE", wxString::Format("Redistributed %d cards", (int)htmls.size()));
    m_html->EndBatchUpdate();
  }

  // ==================== UI helpers ====================

  void UpdateColumnCounts() {
    for (auto &col : m_columns) {
      Box *body = m_html->QuerySelector("#body-" + col);
      Box *cnt = m_html->QuerySelector("#cnt-" + col);
      if (body && cnt) {
        int n = 0;
        for (Box *c : m_html->GetElementChildren(body))
          if (m_html->HasClass(c, "card")) n++;
        m_html->SetTextContent(cnt, wxString::Format("%d", n));
      }
    }
    auto all = m_html->QuerySelectorAll(".card");
    Box *total = m_html->QuerySelector("#total-count");
    if (total)
      m_html->SetTextContent(total, wxString::Format("%d", (int)all.size()));
  }

  void UpdateUI() {
    Box *selInfo = m_html->QuerySelector("#selected-info");
    if (selInfo) {
      if (!m_selectedCardId.empty()) {
        Box *card = m_html->QuerySelector("#" + m_selectedCardId);
        if (card) {
          wxString col = FindCardColumn(m_selectedCardId);
          m_html->SetTextContent(selInfo, GetCardTitle(card) + " (" + ColumnDisplayName(col) + ")");
        }
      } else {
        m_html->SetTextContent(selInfo, "none");
      }
    }

    Box *btnL = m_html->QuerySelector("#btn-left");
    Box *btnR = m_html->QuerySelector("#btn-right");
    Box *btnD = m_html->QuerySelector("#btn-delete");

    bool canL = false, canR = false;
    if (!m_selectedCardId.empty()) {
      int idx = ColumnIndex(FindCardColumn(m_selectedCardId));
      canL = idx > 0;
      canR = idx >= 0 && idx < (int)m_columns.size() - 1;
    }

    auto enable = [this](Box *btn, bool on) {
      if (!btn) return;
      if (on) m_html->RemoveClass(btn, "btn-disabled");
      else    m_html->AddClass(btn, "btn-disabled");
    };
    enable(btnL, canL);
    enable(btnR, canR);
    enable(btnD, !m_selectedCardId.empty());
  }

  void AddLogEntry(const wxString &type, const wxString &message) {
    Box *log = m_html->QuerySelector("#event-log");
    if (!log) return;

    wxString timeStr = wxDateTime::Now().Format("[%H:%M:%S]");
    wxString entryId = wxString::Format("log-%d", m_logCounter++);

    // Color code drag/drop entries
    wxString typeClass = "log-type";
    if (type == "DRAG") typeClass = "log-type log-drag";
    else if (type == "DROP") typeClass = "log-type log-drop";

    wxString entryHTML = wxString::Format(
      "<div class=\"log-entry\" id=\"%s\">"
      "<span class=\"log-time\">%s</span> "
      "<span class=\"%s\">%s</span> %s</div>",
      entryId, timeStr, typeClass, type, message);

    auto entries = m_html->QuerySelectorAll(".log-entry");
    if (entries.size() > 7)
      for (int i = 7; i < (int)entries.size(); i++)
        m_html->RemoveChild(entries[i]);

    wxString header = "<h3>Event Log</h3>";
    auto children = m_html->GetElementChildren(log);
    wxString inner = header + entryHTML;
    for (auto *child : children)
      if (m_html->HasClass(child, "log-entry"))
        inner += m_html->GetOuterHTML(child);
    m_html->SetInnerHTML(log, inner);

    Box *lastAction = m_html->QuerySelector("#last-action");
    if (lastAction)
      m_html->SetTextContent(lastAction, type + ": " + message);
  }
};

class EventsDemoApp : public wxApp {
public:
  bool OnInit() override {
    auto *frame = new EventsDemoFrame();
    frame->Show();
    return true;
  }
};

wxIMPLEMENT_APP(EventsDemoApp);
