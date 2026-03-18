#include "wxHtmlEditWidget.h"
#include <wx/wx.h>
#include <wx/splitter.h>
#include <wx/dcbuffer.h>

// ===== Owner-drawn flat button =====

class FlatButton : public wxWindow {
public:
  wxColour m_bg, m_fg, m_hoverBg, m_borderCol;
  wxString m_label;
  wxFont m_font;
  bool m_hover = false;
  bool m_pressed = false;
  bool m_pill = false;
  bool m_outlined = false;
  bool m_toggled = false;  // toggle state for star, etc.

  FlatButton(wxWindow *parent, wxWindowID id, const wxString &label,
             const wxSize &size = wxSize(70, 28))
      : wxWindow(parent, id, wxDefaultPosition, size),
        m_label(label) {
    m_font = GetFont();
    SetBackgroundStyle(wxBG_STYLE_PAINT);
    SetCursor(wxCURSOR_HAND);
    SetMinSize(size);
    Bind(wxEVT_PAINT, &FlatButton::OnPaint, this);
    Bind(wxEVT_ENTER_WINDOW, [this](wxMouseEvent &) { m_hover = true; Refresh(); });
    Bind(wxEVT_LEAVE_WINDOW, [this](wxMouseEvent &) { m_hover = false; m_pressed = false; Refresh(); });
    Bind(wxEVT_LEFT_DOWN, [this](wxMouseEvent &) { m_pressed = true; Refresh(); });
    Bind(wxEVT_LEFT_UP, [this](wxMouseEvent &evt) {
      m_pressed = false; Refresh();
      if (GetClientRect().Contains(evt.GetPosition())) {
        wxCommandEvent ce(wxEVT_BUTTON, GetId());
        ce.SetEventObject(this);
        ProcessWindowEvent(ce);
      }
    });
  }

  void SetColors(const wxColour &bg, const wxColour &fg, const wxColour &hoverBg) {
    m_bg = bg; m_fg = fg; m_hoverBg = hoverBg;
    Refresh();
  }

  void SetLabel(const wxString &label) override { m_label = label; Refresh(); }
  void SetButtonFont(const wxFont &f) { m_font = f; Refresh(); }

private:
  void OnPaint(wxPaintEvent &) {
    wxAutoBufferedPaintDC dc(this);
    wxRect r = GetClientRect();
    wxColour parentBg = GetParent()->GetBackgroundColour();
    dc.SetBrush(wxBrush(parentBg));
    dc.SetPen(*wxTRANSPARENT_PEN);
    dc.DrawRectangle(r);

    int radius = m_pill ? r.height / 2 : 5;
    wxColour bg = m_hover ? m_hoverBg : m_bg;
    if (m_pressed && bg.IsOk())
      bg = bg.ChangeLightness(m_bg.IsOk() && m_bg.GetLuminance() < 0.5 ? 130 : 85);

    if (m_outlined) {
      dc.SetBrush(m_hover ? wxBrush(m_hoverBg) : *wxTRANSPARENT_BRUSH);
      dc.SetPen(wxPen(m_borderCol.IsOk() ? m_borderCol : m_fg, 1));
      dc.DrawRoundedRectangle(r.Deflate(1), radius);
    } else {
      dc.SetBrush(wxBrush(bg));
      dc.SetPen(*wxTRANSPARENT_PEN);
      dc.DrawRoundedRectangle(r, radius);
    }

    dc.SetFont(m_font);
    dc.SetTextForeground(m_fg);
    wxSize ts = dc.GetTextExtent(m_label);
    dc.DrawText(m_label, (r.width - ts.x) / 2, (r.height - ts.y) / 2);
  }
};

// ===== Label / Tag =====

struct Label {
  const char *name;
  unsigned long color;  // RGB
};

static const Label kLabels[] = {
  {"Work",      0x5B5FC7},
  {"Personal",  0x27AE60},
  {"Urgent",    0xE74C3C},
  {"Finance",   0xF39C12},
  {"Dev",       0x3498DB},
  {"Design",    0x9B59B6},
  {"Team",      0x1ABC9C},
};

// ===== Email data =====

enum Priority { PRI_NONE, PRI_LOW, PRI_NORMAL, PRI_HIGH };

struct EmailEntry {
  const char *from;
  const char *to;
  const char *subject;
  const char *date;
  const char *preview;
  const char *body;
  bool unread;
  bool starred;
  bool hasAttachment;
  Priority priority;
  const char *avatar;
  unsigned long avatarColor;
  int labels[3];  // indices into kLabels, -1 = none
};

static EmailEntry kEmails[] = {
  {
    "Alice Chen", "you@example.com",
    "Re: Project timeline update", "10:32 AM",
    "Thanks for the update. I agree we should push the deadline to next Friday...",
    "<p>Hi,</p>"
    "<p>Thanks for the update. I agree we should push the deadline "
    "to next Friday. I've already told the design team.</p>"
    "<p>One question: should we include the new onboarding flow in "
    "this release, or defer it to v2.1?</p>"
    "<p>Best,<br/>Alice</p>"
    "<blockquote style=\"margin: 16px 0 0 0; padding: 0 0 0 12px; "
    "border-left: 3px solid currentColor; opacity: 0.4;\">"
    "  <p style=\"font-size: 9pt; opacity: 0.6;\">On March 6, 2026, You wrote:</p>"
    "  <p>Just wanted to let you know that we're running a bit behind "
    "on the dashboard redesign.</p>"
    "  <p>I think we should push the milestone to next Friday.</p>"
    "</blockquote>",
    true, true, false, PRI_HIGH, "AC", 0x5B5FC7, {0, 2, -1}
  },
  {
    "Bob Martin", "you@example.com",
    "Code review: PR #247", "9:15 AM",
    "I left a few comments on your PR. Mostly minor stuff \xe2\x80\x94 naming...",
    "<p>Hey,</p>"
    "<p>I left a few comments on your PR. Mostly minor stuff &mdash; "
    "naming conventions and one edge case in the parser.</p>"
    "<p>The overall approach looks solid. Let me know when you've "
    "addressed the feedback and I'll approve.</p>"
    "<p>Bob</p>",
    false, false, false, PRI_NORMAL, "BM", 0xC74B50, {4, -1, -1}
  },
  {
    "CI Pipeline", "dev-team@example.com",
    "Build #1842 passed", "8:44 AM",
    "All 1164 tests passed. Branch: main. Duration: 2m 34s.",
    "<p style=\"color: #27ae60; font-weight: bold;\">Build #1842: SUCCESS</p>"
    "<table style=\"width: 100%; border-collapse: collapse; margin: 12px 0;\">"
    "<tr><td style=\"padding: 6px 12px;\">Branch</td>"
    "<td style=\"padding: 6px 12px;\"><b>main</b></td></tr>"
    "<tr><td style=\"padding: 6px 12px;\">Tests</td>"
    "<td style=\"padding: 6px 12px;\">1164/1164 passed</td></tr>"
    "<tr><td style=\"padding: 6px 12px;\">Duration</td>"
    "<td style=\"padding: 6px 12px;\">2m 34s</td></tr>"
    "</table>",
    false, false, false, PRI_NONE, "CI", 0x27AE60, {4, -1, -1}
  },
  {
    "Carol Davis", "you@example.com",
    "Lunch today?", "Yesterday",
    "Hey! Want to grab lunch today? I was thinking that new Thai place...",
    "<p>Hey! Want to grab lunch today? I was thinking that new Thai "
    "place on Market Street.</p>"
    "<p>Let me know if 12:30 works for you.</p>"
    "<p>Carol</p>",
    false, false, false, PRI_NONE, "CD", 0xE67E22, {1, -1, -1}
  },
  {
    "Dave Wilson", "team@example.com",
    "Q1 Report draft attached", "Yesterday",
    "Please find the Q1 report draft attached. Revenue up 12%...",
    "<p>Hi team,</p>"
    "<p>Please find the Q1 report draft attached. Key highlights:</p>"
    "<ul>"
    "<li>Revenue up 12% quarter-over-quarter</li>"
    "<li>Active users grew to 45K (from 38K)</li>"
    "<li>Infrastructure costs reduced by 8%</li>"
    "</ul>"
    "<p>Please review by Friday.</p>"
    "<p>Thanks,<br/>Dave</p>",
    true, false, true, PRI_NORMAL, "DW", 0x2980B9, {0, 3, -1}
  },
  {
    "Eve Thompson", "engineering@example.com",
    "Meeting notes - March 5", "Mar 5",
    "Sprint Planning notes. Action items for Alice, Bob, You, Frank...",
    "<p><b>Sprint Planning - March 5</b></p>"
    "<p>Attendees: Eve, Alice, Bob, You, Frank</p>"
    "<h3>Action Items</h3>"
    "<ol>"
    "<li>Alice: finalize dashboard wireframes by Wed</li>"
    "<li>Bob: complete API rate limiter implementation</li>"
    "<li>You: address PR #247 review feedback</li>"
    "<li>Frank: set up staging environment</li>"
    "</ol>"
    "<p>Next meeting: March 12 at 10 AM.</p>",
    false, true, true, PRI_NONE, "ET", 0x8E44AD, {0, 6, -1}
  },
  {
    "Frank Lee", "you@example.com",
    "Re: API rate limiting strategy", "Mar 5",
    "I've pushed the initial implementation. Can you take a look at the...",
    "<p>Hey,</p>"
    "<p>I've pushed the initial implementation of the token bucket "
    "rate limiter to the <code>feature/rate-limit</code> branch.</p>"
    "<p>Can you take a look at the middleware integration? I'm not "
    "sure about the header naming convention.</p>"
    "<p>Also, should we support both sliding window and fixed window, "
    "or just start with fixed?</p>"
    "<p>Frank</p>",
    false, false, false, PRI_LOW, "FL", 0x16A085, {4, 0, -1}
  },
  {
    "Grace Kim", "design@example.com",
    "New brand guidelines v2", "Mar 4",
    "Hi team, attached are the updated brand guidelines. Major changes...",
    "<p>Hi team,</p>"
    "<p>Attached are the updated brand guidelines. Major changes:</p>"
    "<ul>"
    "<li>New primary color palette (more accessible)</li>"
    "<li>Updated typography scale</li>"
    "<li>Revised logo usage rules</li>"
    "<li>Dark mode color specifications</li>"
    "</ul>"
    "<p>Please review and let me know if you have any concerns "
    "before we roll these out next week.</p>"
    "<p>Grace</p>",
    false, false, true, PRI_NORMAL, "GK", 0xD35400, {5, 0, -1}
  },
};
static const int kNumEmails = sizeof(kEmails) / sizeof(kEmails[0]);

// ===== Owner-drawn email list =====

class EmailListPanel : public wxWindow {
public:
  wxColour m_bg, m_fg, m_dimFg, m_selBg, m_selFg, m_hoverBg, m_divider;
  int m_selected = 0;
  int m_hovered = -1;
  int m_itemHeight = 82;
  std::function<void(int)> onSelect;
  std::function<void(int)> onStarToggle;

  EmailListPanel(wxWindow *parent) : wxWindow(parent, wxID_ANY) {
    SetBackgroundStyle(wxBG_STYLE_PAINT);
    SetCursor(wxCURSOR_HAND);
    Bind(wxEVT_PAINT, &EmailListPanel::OnPaint, this);
    Bind(wxEVT_LEFT_DOWN, &EmailListPanel::OnClick, this);
    Bind(wxEVT_MOTION, [this](wxMouseEvent &evt) {
      int idx = evt.GetY() / m_itemHeight;
      if (idx != m_hovered) { m_hovered = idx; Refresh(); }
    });
    Bind(wxEVT_LEAVE_WINDOW, [this](wxMouseEvent &) { m_hovered = -1; Refresh(); });
  }

  void SetTheme(const wxColour &bg, const wxColour &fg, const wxColour &dimFg,
                const wxColour &selBg, const wxColour &selFg,
                const wxColour &hoverBg, const wxColour &divider) {
    m_bg = bg; m_fg = fg; m_dimFg = dimFg;
    m_selBg = selBg; m_selFg = selFg;
    m_hoverBg = hoverBg; m_divider = divider;
    Refresh();
  }

private:
  void DrawPill(wxDC &dc, int x, int y, const wxString &text,
                const wxColour &bg, const wxColour &fg, const wxFont &font) {
    dc.SetFont(font);
    wxSize ts = dc.GetTextExtent(text);
    int pw = ts.x + 10;
    int ph = ts.y + 2;
    dc.SetBrush(wxBrush(bg));
    dc.SetPen(*wxTRANSPARENT_PEN);
    dc.DrawRoundedRectangle(x, y, pw, ph, ph / 2);
    dc.SetTextForeground(fg);
    dc.DrawText(text, x + 5, y + 1);
  }

  void OnPaint(wxPaintEvent &) {
    wxAutoBufferedPaintDC dc(this);
    wxRect cr = GetClientRect();
    dc.SetBrush(wxBrush(m_bg));
    dc.SetPen(*wxTRANSPARENT_PEN);
    dc.DrawRectangle(cr);

    wxFont normalFont = GetFont();
    wxFont boldFont = normalFont.Bold();
    wxFont smallFont = normalFont.Scaled(0.85f);
    wxFont tinyFont = normalFont.Scaled(0.78f);

    for (int i = 0; i < kNumEmails; ++i) {
      int y = i * m_itemHeight;
      if (y > cr.height) break;

      const auto &e = kEmails[i];
      bool isSel = (i == m_selected);
      wxRect itemRect(0, y, cr.width, m_itemHeight);

      // Selection / hover
      if (isSel) {
        dc.SetBrush(wxBrush(m_selBg));
        dc.SetPen(*wxTRANSPARENT_PEN);
        dc.DrawRoundedRectangle(itemRect.Deflate(4, 2), 8);
      } else if (i == m_hovered) {
        dc.SetBrush(wxBrush(m_hoverBg));
        dc.SetPen(*wxTRANSPARENT_PEN);
        dc.DrawRoundedRectangle(itemRect.Deflate(4, 2), 8);
      }

      wxColour textFg = isSel ? m_selFg : m_fg;
      wxColour dimFg = isSel ? m_selFg : m_dimFg;

      // Priority indicator bar (left edge)
      if (e.priority == PRI_HIGH) {
        dc.SetBrush(wxBrush(wxColour(0xE7, 0x4C, 0x3C)));
        dc.SetPen(*wxTRANSPARENT_PEN);
        dc.DrawRoundedRectangle(6, y + 8, 3, m_itemHeight - 16, 1);
      } else if (e.priority == PRI_LOW) {
        dc.SetBrush(wxBrush(wxColour(0x95, 0xA5, 0xA6)));
        dc.SetPen(*wxTRANSPARENT_PEN);
        dc.DrawRoundedRectangle(6, y + 8, 3, m_itemHeight - 16, 1);
      }

      int avatarSize = 36;
      int ax = 18;
      int ay = y + 12;
      int textX = ax + avatarSize + 12;

      // Avatar circle
      unsigned long aColor = e.avatarColor;
      wxColour avatarCol((aColor >> 16) & 0xFF, (aColor >> 8) & 0xFF, aColor & 0xFF);
      dc.SetBrush(wxBrush(avatarCol));
      dc.SetPen(*wxTRANSPARENT_PEN);
      dc.DrawCircle(ax + avatarSize / 2, ay + avatarSize / 2, avatarSize / 2);

      dc.SetFont(normalFont);
      dc.SetTextForeground(*wxWHITE);
      wxString initials = e.avatar;
      wxSize is = dc.GetTextExtent(initials);
      dc.DrawText(initials, ax + (avatarSize - is.x) / 2,
                  ay + (avatarSize - is.y) / 2);

      // Unread dot
      if (e.unread) {
        dc.SetBrush(wxBrush(wxColour(0, 120, 212)));
        dc.SetPen(*wxTRANSPARENT_PEN);
        dc.DrawCircle(14, y + m_itemHeight / 2, 4);
      }

      // Row 1: sender + star + date
      dc.SetFont(e.unread ? boldFont : normalFont);
      dc.SetTextForeground(textFg);
      dc.DrawText(e.from, textX, y + 10);

      // Star
      int starX = textX + dc.GetTextExtent(e.from).x + 6;
      if (e.starred) {
        dc.SetTextForeground(wxColour(0xF3, 0x9C, 0x12));
        dc.DrawText("\xE2\x98\x85", starX, y + 10);  // filled star
      }

      // Attachment icon
      if (e.hasAttachment) {
        dc.SetFont(smallFont);
        dc.SetTextForeground(dimFg);
        int attX = cr.width - 50;
        dc.DrawText("\xF0\x9F\x93\x8E", attX, y + 12);  // paperclip
      }

      // Date
      dc.SetFont(smallFont);
      dc.SetTextForeground(dimFg);
      wxString date = e.date;
      wxSize ds = dc.GetTextExtent(date);
      dc.DrawText(date, cr.width - ds.x - 16, y + 12);

      // Row 2: subject
      dc.SetFont(e.unread ? boldFont : normalFont);
      dc.SetTextForeground(textFg);
      int labelAreaW = 0;
      // Pre-calculate label width to clip subject properly
      for (int li = 0; li < 3 && e.labels[li] >= 0; ++li)
        labelAreaW += dc.GetTextExtent(kLabels[e.labels[li]].name).x + 18;
      int subjMaxW = cr.width - textX - 16 - labelAreaW;
      dc.SetClippingRegion(textX, y + 32, subjMaxW, 16);
      dc.DrawText(e.subject, textX, y + 32);
      dc.DestroyClippingRegion();

      // Labels (pill badges on row 2, right side)
      int labelX = cr.width - 16;
      for (int li = 2; li >= 0; --li) {
        if (e.labels[li] < 0) continue;
        const auto &lbl = kLabels[e.labels[li]];
        wxColour lc((lbl.color >> 16) & 0xFF, (lbl.color >> 8) & 0xFF, lbl.color & 0xFF);
        dc.SetFont(tinyFont);
        wxSize ls = dc.GetTextExtent(lbl.name);
        int pw = ls.x + 10;
        labelX -= pw + 4;
        // Pill with semi-transparent background
        wxColour pillBg(lc.Red(), lc.Green(), lc.Blue(), 40);
        DrawPill(dc, labelX, y + 33, lbl.name, pillBg, lc, tinyFont);
      }

      // Row 3: preview
      dc.SetFont(smallFont);
      dc.SetTextForeground(dimFg);
      dc.SetClippingRegion(textX, y + 54, cr.width - textX - 16, 16);
      dc.DrawText(e.preview, textX, y + 54);
      dc.DestroyClippingRegion();

      // Divider
      if (!isSel && i < kNumEmails - 1) {
        dc.SetPen(wxPen(m_divider));
        dc.DrawLine(textX, y + m_itemHeight - 1,
                    cr.width - 16, y + m_itemHeight - 1);
      }
    }
  }

  void OnClick(wxMouseEvent &evt) {
    int idx = evt.GetY() / m_itemHeight;
    if (idx < 0 || idx >= kNumEmails) return;

    // Check if click is on the star area
    // Star is near sender name, roughly textX + senderWidth + 6
    // For simplicity, check if it's in the avatar column area for star toggle
    // Actually let's use a specific region: near the star position
    // For now just select
    if (idx != m_selected) {
      m_selected = idx;
      Refresh();
      if (onSelect) onSelect(idx);
    }
  }
};

// ===== Owner-drawn sidebar =====

class SidebarPanel : public wxWindow {
public:
  struct Item { wxString label; wxString icon; int count; };
  std::vector<Item> m_items;
  int m_selected = 0;
  int m_hovered = -1;
  int m_itemHeight = 34;
  wxColour m_bg, m_fg, m_selBg, m_selFg, m_hoverBg, m_badgeBg, m_badgeFg;
  // Label section
  int m_labelSectionY = 0;

  SidebarPanel(wxWindow *parent) : wxWindow(parent, wxID_ANY) {
    m_items = {
      {"Inbox",     "\xF0\x9F\x93\xA5", 6},
      {"Starred",   "\xE2\x98\x85", 0},
      {"Sent",      "\xE2\x9C\x89", 0},
      {"Drafts",    "\xF0\x9F\x93\x9D", 1},
      {"Archive",   "\xF0\x9F\x93\xA6", 0},
      {"Spam",      "\xE2\x9A\xA0", 2},
      {"Trash",     "\xF0\x9F\x97\x91", 0},
    };
    SetBackgroundStyle(wxBG_STYLE_PAINT);
    SetMinSize(wxSize(190, -1));
    SetCursor(wxCURSOR_HAND);
    Bind(wxEVT_PAINT, &SidebarPanel::OnPaint, this);
    Bind(wxEVT_LEFT_DOWN, [this](wxMouseEvent &evt) {
      int idx = (evt.GetY() - 12) / m_itemHeight;
      if (idx >= 0 && idx < (int)m_items.size()) { m_selected = idx; Refresh(); }
    });
    Bind(wxEVT_MOTION, [this](wxMouseEvent &evt) {
      int idx = (evt.GetY() - 12) / m_itemHeight;
      if (idx != m_hovered) { m_hovered = idx; Refresh(); }
    });
    Bind(wxEVT_LEAVE_WINDOW, [this](wxMouseEvent &) { m_hovered = -1; Refresh(); });
  }

  void SetTheme(const wxColour &bg, const wxColour &fg,
                const wxColour &selBg, const wxColour &selFg,
                const wxColour &hoverBg,
                const wxColour &badgeBg, const wxColour &badgeFg) {
    m_bg = bg; m_fg = fg; m_selBg = selBg; m_selFg = selFg;
    m_hoverBg = hoverBg; m_badgeBg = badgeBg; m_badgeFg = badgeFg;
    Refresh();
  }

private:
  void OnPaint(wxPaintEvent &) {
    wxAutoBufferedPaintDC dc(this);
    wxRect cr = GetClientRect();
    dc.SetBrush(wxBrush(m_bg));
    dc.SetPen(*wxTRANSPARENT_PEN);
    dc.DrawRectangle(cr);

    wxFont labelFont = GetFont();
    wxFont badgeFont = labelFont.Scaled(0.85f);
    wxFont sectionFont = labelFont.Scaled(0.8f).Bold();
    wxFont tagFont = labelFont.Scaled(0.9f);

    int topPad = 12;

    // Folder items
    for (int i = 0; i < (int)m_items.size(); ++i) {
      int y = topPad + i * m_itemHeight;
      wxRect ir(6, y, cr.width - 12, m_itemHeight);

      bool isSel = (i == m_selected);
      if (isSel) {
        dc.SetBrush(wxBrush(m_selBg));
        dc.SetPen(*wxTRANSPARENT_PEN);
        dc.DrawRoundedRectangle(ir, 6);
      } else if (i == m_hovered) {
        dc.SetBrush(wxBrush(m_hoverBg));
        dc.SetPen(*wxTRANSPARENT_PEN);
        dc.DrawRoundedRectangle(ir, 6);
      }

      dc.SetFont(labelFont);
      dc.SetTextForeground(isSel ? m_selFg : m_fg);
      dc.DrawText(m_items[i].icon, 18, y + (m_itemHeight - dc.GetCharHeight()) / 2);
      dc.DrawText(m_items[i].label, 44, y + (m_itemHeight - dc.GetCharHeight()) / 2);

      if (m_items[i].count > 0) {
        dc.SetFont(badgeFont);
        wxString cnt = wxString::Format("%d", m_items[i].count);
        wxSize cs = dc.GetTextExtent(cnt);
        int bw = std::max(cs.x + 12, 22);
        int bx = cr.width - bw - 12;
        int by = y + (m_itemHeight - 18) / 2;
        dc.SetBrush(wxBrush(m_badgeBg));
        dc.SetPen(*wxTRANSPARENT_PEN);
        dc.DrawRoundedRectangle(bx, by, bw, 18, 9);
        dc.SetTextForeground(m_badgeFg);
        dc.DrawText(cnt, bx + (bw - cs.x) / 2, by + (18 - cs.y) / 2);
      }
    }

    // Labels section header
    int secY = topPad + (int)m_items.size() * m_itemHeight + 16;
    dc.SetFont(sectionFont);
    dc.SetTextForeground(m_fg.IsOk() ? m_fg.ChangeLightness(140) : wxColour(150, 150, 150));
    dc.DrawText("LABELS", 18, secY);
    secY += 24;

    // Label items
    int numLabels = sizeof(kLabels) / sizeof(kLabels[0]);
    for (int i = 0; i < numLabels; ++i) {
      int y = secY + i * 28;
      const auto &lbl = kLabels[i];
      wxColour lc((lbl.color >> 16) & 0xFF, (lbl.color >> 8) & 0xFF, lbl.color & 0xFF);

      // Color dot
      dc.SetBrush(wxBrush(lc));
      dc.SetPen(*wxTRANSPARENT_PEN);
      dc.DrawCircle(26, y + 12, 5);

      // Label name
      dc.SetFont(tagFont);
      dc.SetTextForeground(m_fg);
      dc.DrawText(lbl.name, 40, y + 4);
    }
  }
};

// ===== HTML builders =====

static wxString BuildEmailHTML(const EmailEntry &e) {
  wxString html;
  html += "<body style=\"font-family: -apple-system, Segoe UI, Helvetica, "
          "Arial, sans-serif; font-size: 10.5pt; padding: 24px; line-height: 1.6;\">";

  // Header area
  html += "<div style=\"margin-bottom: 20px; padding-bottom: 16px;\">";

  // Priority + subject line
  if (e.priority == PRI_HIGH)
    html += "<span style=\"color: #E74C3C; font-size: 9pt; font-weight: bold; "
            "margin-right: 8px;\">HIGH PRIORITY</span>";

  html += wxString::Format(
      "<div style=\"font-size: 16pt; font-weight: bold; margin-bottom: 8px;\">%s</div>",
      e.subject);

  // From / To
  html += wxString::Format(
      "<div style=\"font-size: 9pt; opacity: 0.5; margin-bottom: 6px;\">"
      "%s &middot; to %s &middot; %s</div>",
      e.from, e.to, e.date);

  // Labels
  bool hasLabel = false;
  for (int li = 0; li < 3 && e.labels[li] >= 0; ++li) {
    const auto &lbl = kLabels[e.labels[li]];
    if (!hasLabel) { html += "<div style=\"margin-top: 6px;\">"; hasLabel = true; }
    html += wxString::Format(
        "<span style=\"display: inline-block; font-size: 8pt; "
        "padding: 2px 8px; margin-right: 4px; border-radius: 10px; "
        "background: rgba(%d,%d,%d,0.15); color: #%06lX;\">%s</span>",
        (int)((lbl.color >> 16) & 0xFF), (int)((lbl.color >> 8) & 0xFF),
        (int)(lbl.color & 0xFF), lbl.color, lbl.name);
  }
  if (hasLabel) html += "</div>";

  // Attachment indicator
  if (e.hasAttachment)
    html += "<div style=\"font-size: 9pt; opacity: 0.5; margin-top: 6px;\">"
            "\xF0\x9F\x93\x8E 1 attachment</div>";

  html += "</div>";

  html += e.body;
  html += "</body>";
  return html;
}

static wxString BuildReplyHTML(const EmailEntry &e) {
  wxString html;
  html += "<body style=\"font-family: -apple-system, Segoe UI, Helvetica, "
          "Arial, sans-serif; font-size: 10.5pt; padding: 16px; line-height: 1.6;\">";
  html += "<p><br/></p>";
  html += wxString::Format(
      "<div style=\"font-size: 9pt; opacity: 0.5; margin: 20px 0 6px 0;\">"
      "On %s, %s wrote:</div>", e.date, e.from);
  html += "<blockquote style=\"margin: 0; padding: 0 0 0 14px; "
          "border-left: 3px solid currentColor; opacity: 0.4;\">";
  html += e.body;
  html += "</blockquote></body>";
  return html;
}

// ===== Theme =====

struct Theme {
  wxColour sidebarBg, sidebarFg, sidebarSelBg, sidebarSelFg, sidebarHoverBg;
  wxColour badgeBg, badgeFg;
  wxColour toolbarBg, toolbarFg, toolbarHover;
  wxColour listBg, listFg, listDimFg, listSelBg, listSelFg, listHoverBg, listDivider;
  wxColour editorBg, editorFg;
  wxColour inputBg, inputFg, labelFg;
};

static const Theme kLight = {
  wxColour(245, 245, 245), wxColour(60, 60, 60),
  wxColour(0, 120, 212), *wxWHITE,
  wxColour(232, 232, 232),
  wxColour(0, 120, 212), *wxWHITE,
  wxColour(255, 255, 255), wxColour(50, 50, 50), wxColour(240, 240, 240),
  wxColour(255, 255, 255), wxColour(30, 30, 30), wxColour(130, 130, 130),
  wxColour(232, 240, 254), wxColour(30, 30, 30),
  wxColour(248, 248, 248), wxColour(235, 235, 235),
  wxColour(255, 255, 255), wxColour(30, 30, 30),
  wxColour(255, 255, 255), wxColour(30, 30, 30), wxColour(100, 100, 100),
};

static const Theme kDark = {
  wxColour(25, 25, 25), wxColour(180, 180, 180),
  wxColour(55, 55, 60), wxColour(230, 230, 230),
  wxColour(40, 40, 42),
  wxColour(0, 120, 212), *wxWHITE,
  wxColour(32, 32, 32), wxColour(210, 210, 210), wxColour(50, 50, 54),
  wxColour(25, 25, 25), wxColour(220, 220, 220), wxColour(130, 130, 130),
  wxColour(40, 50, 65), wxColour(220, 220, 220),
  wxColour(35, 35, 38), wxColour(45, 45, 48),
  wxColour(25, 25, 25), wxColour(215, 215, 215),
  wxColour(35, 35, 38), wxColour(210, 210, 210), wxColour(150, 150, 150),
};

// ===== App =====

class EmailDemoApp : public wxApp {
public:
  bool OnInit() override {
    wxInitAllImageHandlers();
    auto *frame = new wxFrame(nullptr, wxID_ANY, "Flux Mail",
                              wxDefaultPosition, wxSize(1280, 850));

    auto *mainPanel = new wxPanel(frame);
    mainPanel->SetBackgroundColour(kLight.toolbarBg);
    auto *mainSizer = new wxBoxSizer(wxHORIZONTAL);

    // ===== Sidebar =====
    auto *sidebar = new SidebarPanel(mainPanel);
    sidebar->SetTheme(kLight.sidebarBg, kLight.sidebarFg,
                      kLight.sidebarSelBg, kLight.sidebarSelFg,
                      kLight.sidebarHoverBg,
                      kLight.badgeBg, kLight.badgeFg);

    // ===== Center column =====
    auto *centerCol = new wxPanel(mainPanel);
    centerCol->SetBackgroundColour(kLight.listBg);
    auto *ccSizer = new wxBoxSizer(wxVERTICAL);

    auto *toolbar = new wxPanel(centerCol);
    toolbar->SetBackgroundColour(kLight.toolbarBg);
    auto *tbSizer = new wxBoxSizer(wxHORIZONTAL);

    auto *btnCompose = new FlatButton(toolbar, wxID_ANY, "+ Compose", wxSize(100, 32));
    btnCompose->m_pill = true;
    btnCompose->SetColors(wxColour(0, 120, 212), *wxWHITE, wxColour(0, 100, 190));

    auto *btnDark = new FlatButton(toolbar, wxID_ANY, "\xE2\x98\xBD", wxSize(32, 28));
    btnDark->m_outlined = true;
    btnDark->m_borderCol = wxColour(180, 180, 180);

    tbSizer->Add(btnCompose, 0, wxALL, 6);
    tbSizer->AddStretchSpacer();
    tbSizer->Add(btnDark, 0, wxALL | wxALIGN_CENTER_VERTICAL, 6);
    toolbar->SetSizer(tbSizer);

    ccSizer->Add(toolbar, 0, wxEXPAND);

    auto *emailList = new EmailListPanel(centerCol);
    emailList->SetTheme(kLight.listBg, kLight.listFg, kLight.listDimFg,
                        kLight.listSelBg, kLight.listSelFg,
                        kLight.listHoverBg, kLight.listDivider);
    ccSizer->Add(emailList, 1, wxEXPAND);
    centerCol->SetSizer(ccSizer);
    centerCol->SetMinSize(wxSize(400, -1));

    // ===== Right column =====
    auto *rightCol = new wxPanel(mainPanel);
    rightCol->SetBackgroundColour(kLight.editorBg);
    auto *rcSizer = new wxBoxSizer(wxVERTICAL);

    auto *previewBar = new wxPanel(rightCol);
    previewBar->SetBackgroundColour(kLight.toolbarBg);
    auto *pbSizer = new wxBoxSizer(wxHORIZONTAL);

    auto *btnReply = new FlatButton(previewBar, wxID_ANY, "Reply");
    auto *btnReplyAll = new FlatButton(previewBar, wxID_ANY, "Reply All", wxSize(80, 28));
    auto *btnForward = new FlatButton(previewBar, wxID_ANY, "Forward");
    auto *btnStar = new FlatButton(previewBar, wxID_ANY, "\xE2\x98\x86", wxSize(30, 28));
    auto *btnArchive = new FlatButton(previewBar, wxID_ANY, "Archive");
    auto *btnTrash = new FlatButton(previewBar, wxID_ANY, "Trash");

    auto setPreviewBarTheme = [=](const Theme &t) {
      previewBar->SetBackgroundColour(t.toolbarBg);
      btnReply->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnReplyAll->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnForward->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnStar->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnArchive->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnTrash->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      previewBar->Refresh();
    };
    setPreviewBarTheme(kLight);

    pbSizer->Add(btnReply, 0, wxALL, 4);
    pbSizer->Add(btnReplyAll, 0, wxALL, 4);
    pbSizer->Add(btnForward, 0, wxALL, 4);
    pbSizer->AddSpacer(8);
    pbSizer->Add(btnStar, 0, wxALL, 4);
    pbSizer->AddStretchSpacer();
    pbSizer->Add(btnArchive, 0, wxALL, 4);
    pbSizer->Add(btnTrash, 0, wxALL, 4);
    previewBar->SetSizer(pbSizer);

    auto *previewWidget = new wxHtmlEditWidget(rightCol);
    previewWidget->SetReadOnly(true);
    previewWidget->GetEngine().defaultFont =
        wxFont(10, wxFONTFAMILY_DEFAULT, wxFONTSTYLE_NORMAL, wxFONTWEIGHT_NORMAL);
    previewWidget->SetHTML(BuildEmailHTML(kEmails[0]));

    // Compose area
    auto *composeBox = new wxPanel(rightCol);
    composeBox->SetBackgroundColour(kLight.toolbarBg);
    auto *cbSizer = new wxBoxSizer(wxVERTICAL);

    auto *composeHeader = new wxPanel(composeBox);
    composeHeader->SetBackgroundColour(kLight.toolbarBg);
    auto *chSizer = new wxFlexGridSizer(2, 2, 6, 8);
    chSizer->AddGrowableCol(1, 1);
    auto *lblTo = new wxStaticText(composeHeader, wxID_ANY, "To");
    auto *txtTo = new wxTextCtrl(composeHeader, wxID_ANY, "");
    auto *lblSubj = new wxStaticText(composeHeader, wxID_ANY, "Subject");
    auto *txtSubj = new wxTextCtrl(composeHeader, wxID_ANY, "");
    chSizer->Add(lblTo, 0, wxALIGN_CENTER_VERTICAL);
    chSizer->Add(txtTo, 1, wxEXPAND);
    chSizer->Add(lblSubj, 0, wxALIGN_CENTER_VERTICAL);
    chSizer->Add(txtSubj, 1, wxEXPAND);
    composeHeader->SetSizer(chSizer);

    auto *cToolbar = new wxPanel(composeBox);
    cToolbar->SetBackgroundColour(kLight.toolbarBg);
    auto *ctbSizer = new wxBoxSizer(wxHORIZONTAL);

    auto *btnSend = new FlatButton(cToolbar, wxID_ANY, "Send", wxSize(80, 32));
    btnSend->m_pill = true;
    btnSend->SetColors(wxColour(0, 120, 212), *wxWHITE, wxColour(0, 100, 190));
    auto *btnDiscard = new FlatButton(cToolbar, wxID_ANY, "Discard");
    auto *btnBold = new FlatButton(cToolbar, wxID_ANY, "B", wxSize(30, 28));
    btnBold->SetButtonFont(btnBold->GetFont().Bold());
    auto *btnItalic = new FlatButton(cToolbar, wxID_ANY, "I", wxSize(30, 28));
    btnItalic->SetButtonFont(btnItalic->GetFont().Italic());
    auto *btnUnderline = new FlatButton(cToolbar, wxID_ANY, "U", wxSize(30, 28));
    auto *btnQuote = new FlatButton(cToolbar, wxID_ANY, "\xC2\xBB", wxSize(30, 28));
    auto *btnUnquote = new FlatButton(cToolbar, wxID_ANY, "\xC2\xAB", wxSize(30, 28));

    auto setComposeToolbarTheme = [=](const Theme &t) {
      cToolbar->SetBackgroundColour(t.toolbarBg);
      btnDiscard->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnBold->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnItalic->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnUnderline->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnQuote->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnUnquote->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
      btnSend->SetColors(wxColour(0, 120, 212), *wxWHITE, wxColour(0, 100, 190));
      cToolbar->Refresh();
    };
    setComposeToolbarTheme(kLight);

    ctbSizer->Add(btnSend, 0, wxALL, 4);
    ctbSizer->Add(btnDiscard, 0, wxALL, 4);
    ctbSizer->AddSpacer(16);
    ctbSizer->Add(btnBold, 0, wxALL, 4);
    ctbSizer->Add(btnItalic, 0, wxALL, 4);
    ctbSizer->Add(btnUnderline, 0, wxALL, 4);
    ctbSizer->AddSpacer(8);
    ctbSizer->Add(btnQuote, 0, wxALL, 4);
    ctbSizer->Add(btnUnquote, 0, wxALL, 4);
    cToolbar->SetSizer(ctbSizer);

    auto *composeWidget = new wxHtmlEditWidget(composeBox);
    composeWidget->SetReadOnly(false);
    composeWidget->GetEngine().defaultFont =
        wxFont(10, wxFONTFAMILY_DEFAULT, wxFONTSTYLE_NORMAL, wxFONTWEIGHT_NORMAL);

    cbSizer->Add(composeHeader, 0, wxEXPAND | wxALL, 8);
    cbSizer->Add(cToolbar, 0, wxEXPAND);
    cbSizer->Add(composeWidget, 1, wxEXPAND);
    composeBox->SetSizer(cbSizer);
    composeBox->Hide();

    rcSizer->Add(previewBar, 0, wxEXPAND);
    rcSizer->Add(previewWidget, 1, wxEXPAND);
    rcSizer->Add(composeBox, 1, wxEXPAND);
    rightCol->SetSizer(rcSizer);

    mainSizer->Add(sidebar, 0, wxEXPAND);
    mainSizer->Add(centerCol, 0, wxEXPAND);
    mainSizer->Add(rightCol, 1, wxEXPAND);
    mainPanel->SetSizer(mainSizer);

    // ===== Events =====

    // Update star button when selection changes
    auto updateStarBtn = [btnStar, emailList]() {
      int sel = emailList->m_selected;
      if (sel >= 0 && sel < kNumEmails && kEmails[sel].starred)
        btnStar->SetLabel("\xE2\x98\x85");  // filled
      else
        btnStar->SetLabel("\xE2\x98\x86");  // outline
    };

    emailList->onSelect = [previewWidget, updateStarBtn](int idx) {
      previewWidget->SetHTML(BuildEmailHTML(kEmails[idx]));
      updateStarBtn();
    };

    // Star toggle
    btnStar->Bind(wxEVT_BUTTON, [emailList, btnStar, updateStarBtn](wxCommandEvent &) {
      int sel = emailList->m_selected;
      if (sel >= 0 && sel < kNumEmails) {
        kEmails[sel].starred = !kEmails[sel].starred;
        updateStarBtn();
        emailList->Refresh();
      }
    });

    // Reply
    btnReply->Bind(wxEVT_BUTTON,
        [emailList, previewWidget, previewBar, composeWidget,
         composeBox, txtTo, txtSubj, rcSizer](wxCommandEvent &) {
      int sel = emailList->m_selected;
      if (sel < 0 || sel >= kNumEmails) return;
      const auto &e = kEmails[sel];
      txtTo->SetValue(e.from);
      wxString subj = e.subject;
      if (!subj.StartsWith("Re: ")) subj = "Re: " + subj;
      txtSubj->SetValue(subj);
      composeWidget->SetHTML(BuildReplyHTML(e));
      previewWidget->wxWindow::Hide();
      previewBar->Hide();
      composeBox->Show();
      rcSizer->Layout();
      composeWidget->SetFocus();
    });

    // Compose
    btnCompose->Bind(wxEVT_BUTTON,
        [previewWidget, previewBar, composeWidget,
         composeBox, txtTo, txtSubj, rcSizer](wxCommandEvent &) {
      txtTo->SetValue("");
      txtSubj->SetValue("");
      composeWidget->SetHTML(
          "<body style=\"font-family: -apple-system, Segoe UI, Helvetica, "
          "Arial, sans-serif; font-size: 10.5pt; padding: 16px; line-height: 1.6;\">"
          "<p><br/></p></body>");
      previewWidget->wxWindow::Hide();
      previewBar->Hide();
      composeBox->Show();
      rcSizer->Layout();
      composeWidget->SetFocus();
    });

    // Discard
    btnDiscard->Bind(wxEVT_BUTTON,
        [previewWidget, previewBar, composeBox, rcSizer](wxCommandEvent &) {
      composeBox->Hide();
      previewWidget->wxWindow::Show();
      previewBar->Show();
      rcSizer->Layout();
    });

    // Formatting
    btnBold->Bind(wxEVT_BUTTON, [composeWidget](wxCommandEvent &) {
      composeWidget->ToggleBold(); composeWidget->SetFocus();
    });
    btnItalic->Bind(wxEVT_BUTTON, [composeWidget](wxCommandEvent &) {
      composeWidget->ToggleItalic(); composeWidget->SetFocus();
    });
    btnUnderline->Bind(wxEVT_BUTTON, [composeWidget](wxCommandEvent &) {
      composeWidget->ToggleUnderline(); composeWidget->SetFocus();
    });
    btnQuote->Bind(wxEVT_BUTTON, [composeWidget](wxCommandEvent &) {
      composeWidget->IncreaseQuoteLevel(); composeWidget->SetFocus();
    });
    btnUnquote->Bind(wxEVT_BUTTON, [composeWidget](wxCommandEvent &) {
      composeWidget->DecreaseQuoteLevel(); composeWidget->SetFocus();
    });

    // Send
    btnSend->Bind(wxEVT_BUTTON,
        [composeWidget, previewWidget, previewBar, composeBox,
         rcSizer, frame](wxCommandEvent &) {
      composeBox->Hide();
      previewWidget->wxWindow::Show();
      previewBar->Show();
      rcSizer->Layout();
      wxMessageBox("Message sent!", "Flux Mail", wxOK | wxICON_INFORMATION, frame);
    });

    // ===== Dark mode =====
    auto *isDark = new bool(false);

    auto setDarkBtnTheme = [=](const Theme &t) {
      btnDark->m_borderCol = t.toolbarFg;
      btnDark->SetColors(t.toolbarBg, t.toolbarFg, t.toolbarHover);
    };
    setDarkBtnTheme(kLight);

    btnDark->Bind(wxEVT_BUTTON,
        [=](wxCommandEvent &) {
      *isDark = !*isDark;
      const Theme &t = *isDark ? kDark : kLight;
      auto scheme = *isDark ? wxHtmlEditWidget::ColorScheme::Dark
                            : wxHtmlEditWidget::ColorScheme::Light;

      btnDark->SetLabel(*isDark ? "\xE2\x98\x80" : "\xE2\x98\xBD");

      sidebar->SetTheme(t.sidebarBg, t.sidebarFg,
                        t.sidebarSelBg, t.sidebarSelFg,
                        t.sidebarHoverBg, t.badgeBg, t.badgeFg);

      toolbar->SetBackgroundColour(t.toolbarBg);
      btnCompose->SetColors(wxColour(0, 120, 212), *wxWHITE, wxColour(0, 100, 190));
      setDarkBtnTheme(t);
      centerCol->SetBackgroundColour(t.listBg);
      toolbar->Refresh();

      emailList->SetTheme(t.listBg, t.listFg, t.listDimFg,
                          t.listSelBg, t.listSelFg,
                          t.listHoverBg, t.listDivider);

      setPreviewBarTheme(t);
      rightCol->SetBackgroundColour(t.editorBg);

      composeHeader->SetBackgroundColour(t.toolbarBg);
      lblTo->SetForegroundColour(t.labelFg);
      lblSubj->SetForegroundColour(t.labelFg);
      txtTo->SetBackgroundColour(t.inputBg);
      txtTo->SetForegroundColour(t.inputFg);
      txtSubj->SetBackgroundColour(t.inputBg);
      txtSubj->SetForegroundColour(t.inputFg);
      composeHeader->Refresh();
      setComposeToolbarTheme(t);
      composeBox->SetBackgroundColour(t.toolbarBg);
      mainPanel->SetBackgroundColour(t.toolbarBg);

      previewWidget->GetEngine().bgColor = t.editorBg;
      previewWidget->GetEngine().defaultColor = t.editorFg;
      previewWidget->SetColorScheme(scheme);
      composeWidget->GetEngine().bgColor = t.editorBg;
      composeWidget->GetEngine().defaultColor = t.editorFg;
      composeWidget->SetColorScheme(scheme);

      frame->Refresh();
    });

    frame->Show();
    return true;
  }
};

wxIMPLEMENT_APP(EmailDemoApp);
