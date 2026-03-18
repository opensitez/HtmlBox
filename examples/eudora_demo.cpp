#include "wxHtmlEditWidget.h"
#include <wx/wx.h>
#include <wx/splitter.h>
#include <wx/dcbuffer.h>
#include <algorithm>
#include <functional>
#include <wx/colordlg.h>

// ===== Theme =====
struct Theme {
  wxColour panelBg, panelFg, panelDim;
  wxColour toolbarBg, toolbarFg, toolbarHover, toolbarBorder;
  wxColour wazooTabBg, wazooTabFg, wazooTabSelBg, wazooTabSelFg;
  wxColour treeBg, treeFg, treeSelBg, treeSelFg, treeHoverBg, treeDim, treeLine;
  wxColour listBg, listFg, listDim, listSelBg, listSelFg, listHoverBg;
  wxColour listHeaderBg, listHeaderFg, listGrid, listUnread;
  wxColour listStripe;
  wxColour editorBg, editorFg;
  wxColour statusBg, statusFg, statusBorder;
  wxColour headerBg, headerFg, headerDim, headerBorder;
  wxColour inputBg, inputFg, labelFg;
  wxColour accentBg, accentFg, accentHover;
  wxColour enabledCol, disabledCol;
  wxColour tabBg, tabSelBg, tabFg, tabSelFg, tabBorder;
  wxColour cardBg, cardBorder, cardShadow;
  wxColour greenBg, redBg, orangeBg;
};

static const Theme kLight = {
  wxColour(240,238,235), wxColour(35,30,25), wxColour(130,125,118),
  wxColour(245,243,240), wxColour(35,30,25), wxColour(230,227,222), wxColour(210,207,202),
  wxColour(230,227,222), wxColour(95,90,83), wxColour(50,110,165), *wxWHITE,
  wxColour(248,247,244), wxColour(35,30,25), wxColour(50,110,165), *wxWHITE,
  wxColour(238,236,232), wxColour(120,115,108), wxColour(215,212,207),
  *wxWHITE, wxColour(35,30,25), wxColour(120,115,108),
  wxColour(50,110,165), *wxWHITE, wxColour(244,242,239),
  wxColour(238,236,232), wxColour(80,75,68), wxColour(222,219,214),
  wxColour(15,10,5),
  wxColour(252,251,249),
  *wxWHITE, wxColour(25,20,15),
  wxColour(245,243,240), wxColour(90,85,78), wxColour(215,212,207),
  wxColour(248,247,244), wxColour(35,30,25), wxColour(110,105,98), wxColour(222,219,214),
  *wxWHITE, wxColour(25,20,15), wxColour(100,95,88),
  wxColour(50,110,165), *wxWHITE, wxColour(40,95,148),
  wxColour(50,110,165), wxColour(165,160,153),
  wxColour(235,232,228), wxColour(252,251,249), wxColour(100,95,88), wxColour(25,20,15), wxColour(210,207,202),
  *wxWHITE, wxColour(225,222,218), wxColour(200,197,192),
  wxColour(46,160,67), wxColour(210,55,45), wxColour(210,153,34),
};

static const Theme kDark = {
  wxColour(30,30,33), wxColour(205,200,193), wxColour(110,105,98),
  wxColour(26,26,29), wxColour(205,200,193), wxColour(42,42,47), wxColour(50,50,55),
  wxColour(22,22,25), wxColour(130,125,118), wxColour(50,85,130), wxColour(225,220,213),
  wxColour(20,20,23), wxColour(200,195,188), wxColour(50,85,130), wxColour(225,220,213),
  wxColour(32,32,36), wxColour(110,105,98), wxColour(40,40,44),
  wxColour(18,18,21), wxColour(200,195,188), wxColour(110,105,98),
  wxColour(45,78,120), wxColour(230,225,218), wxColour(26,26,30),
  wxColour(24,24,28), wxColour(135,130,123), wxColour(38,38,42),
  wxColour(230,225,218),
  wxColour(22,22,26),
  wxColour(18,18,21), wxColour(210,205,198),
  wxColour(24,24,27), wxColour(125,120,113), wxColour(42,42,46),
  wxColour(22,22,25), wxColour(200,195,188), wxColour(95,90,83), wxColour(40,40,44),
  wxColour(28,28,31), wxColour(200,195,188), wxColour(115,110,103),
  wxColour(50,110,165), *wxWHITE, wxColour(40,95,148),
  wxColour(50,110,165), wxColour(70,65,58),
  wxColour(22,22,25), wxColour(32,32,36), wxColour(125,120,113), wxColour(215,210,203), wxColour(42,42,46),
  wxColour(32,32,36), wxColour(44,44,48), wxColour(50,50,55),
  wxColour(46,160,67), wxColour(210,55,45), wxColour(210,153,34),
};

// ===== Toolbar button =====
class TBtn : public wxWindow {
public:
  wxColour m_bg, m_fg, m_hoverBg, m_pressedBg;
  wxString m_label, m_icon;
  wxFont m_font;
  bool m_hover=false, m_pressed=false, m_sep=false, m_accent=false;

  TBtn(wxWindow *p, wxWindowID id, const wxString &lbl, const wxString &ico="",
       const wxSize &sz=wxSize(-1,36))
      : wxWindow(p,id,wxDefaultPosition,sz), m_label(lbl), m_icon(ico) {
    m_font = GetFont();
    SetBackgroundStyle(wxBG_STYLE_PAINT); SetCursor(wxCURSOR_HAND);
    wxClientDC dc(this); dc.SetFont(m_font);
    int w = dc.GetTextExtent(lbl).x + (ico.empty()?0:18) + 24;
    SetMinSize(wxSize(std::max(w,36),sz.GetHeight()));
    Bind(wxEVT_PAINT, &TBtn::OnPaint, this);
    Bind(wxEVT_ENTER_WINDOW, [this](wxMouseEvent&) { m_hover=true; Refresh(); });
    Bind(wxEVT_LEAVE_WINDOW, [this](wxMouseEvent&) { m_hover=m_pressed=false; Refresh(); });
    Bind(wxEVT_LEFT_DOWN, [this](wxMouseEvent&) { m_pressed=true; Refresh(); });
    Bind(wxEVT_LEFT_UP, [this](wxMouseEvent &e) {
      m_pressed=false; Refresh();
      if (GetClientRect().Contains(e.GetPosition())) {
        wxCommandEvent ce(wxEVT_BUTTON,GetId()); ce.SetEventObject(this); ProcessWindowEvent(ce);
      }
    });
  }
  void SetColors(const wxColour &bg, const wxColour &fg, const wxColour &hov) {
    m_bg=bg; m_fg=fg; m_hoverBg=hov; m_pressedBg=hov.ChangeLightness(85); Refresh();
  }
  void SetLabel(const wxString &l) override { m_label=l; Refresh(); }
private:
  void OnPaint(wxPaintEvent&) {
    wxAutoBufferedPaintDC dc(this); wxRect r=GetClientRect();
    dc.SetBrush(wxBrush(GetParent()->GetBackgroundColour()));
    dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(r);
    if (m_accent) {
      dc.SetBrush(wxBrush(m_pressed?m_pressedBg:m_hover?m_hoverBg:m_bg));
      dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRoundedRectangle(r.Deflate(2,4),6);
    } else if (m_hover||m_pressed) {
      dc.SetBrush(wxBrush(m_pressed?m_pressedBg:m_hoverBg));
      dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRoundedRectangle(r.Deflate(2,4),6);
    }
    dc.SetFont(m_font); dc.SetTextForeground(m_accent?*wxWHITE:m_fg);
    int tw=0;
    if (!m_icon.empty()) tw+=dc.GetTextExtent(m_icon).x+5;
    if (!m_label.empty()) tw+=dc.GetTextExtent(m_label).x;
    int x=std::max((r.width-tw)/2,4), y=(r.height-dc.GetCharHeight())/2;
    if (!m_icon.empty()) { dc.DrawText(m_icon,x,y); x+=dc.GetTextExtent(m_icon).x+5; }
    if (!m_label.empty()) dc.DrawText(m_label,x,y);
    if (m_sep) {
      dc.SetPen(wxPen(wxColour(m_fg.Red(),m_fg.Green(),m_fg.Blue(),30)));
      dc.DrawLine(r.width-1,8,r.width-1,r.height-8);
    }
  }
};

// ===== Tab bar =====
class TabBar : public wxWindow {
public:
  struct Tab { wxString label; bool closable; };
  std::vector<Tab> m_tabs;
  int m_sel=0, m_hov=-1, m_hovClose=-1, m_tabH=28;
  wxColour m_bg, m_selBg, m_fg, m_selFg, m_border, m_accentBg;
  std::function<void(int)> onSelect, onClose;

  TabBar(wxWindow *p) : wxWindow(p,wxID_ANY) {
    SetBackgroundStyle(wxBG_STYLE_PAINT); SetMinSize(wxSize(-1,m_tabH)); SetCursor(wxCURSOR_HAND);
    Bind(wxEVT_PAINT, &TabBar::OnPaint, this);
    Bind(wxEVT_LEFT_DOWN, &TabBar::OnClick, this);
    Bind(wxEVT_MOTION, [this](wxMouseEvent &e) {
      int i=TabAtX(e.GetX()), c=CloseAtXY(e.GetX(),e.GetY());
      if (i!=m_hov||c!=m_hovClose) { m_hov=i; m_hovClose=c; Refresh(); }
    });
    Bind(wxEVT_LEAVE_WINDOW, [this](wxMouseEvent&) { m_hov=-1; m_hovClose=-1; Refresh(); });
  }
  void SetTheme(const Theme &t) {
    m_bg=t.tabBg; m_selBg=t.tabSelBg; m_fg=t.tabFg; m_selFg=t.tabSelFg; m_border=t.tabBorder; m_accentBg=t.accentBg; Refresh();
  }
  int AddTab(const wxString &label, bool closable=false) { m_tabs.push_back({label,closable}); Refresh(); return (int)m_tabs.size()-1; }
  void RemoveTab(int idx) {
    if (idx>=0&&idx<(int)m_tabs.size()) {
      m_tabs.erase(m_tabs.begin()+idx);
      if (m_sel>=(int)m_tabs.size()) m_sel=(int)m_tabs.size()-1;
      if (m_sel<0) m_sel=0; Refresh();
    }
  }
  void SelectTab(int idx) { if (idx>=0&&idx<(int)m_tabs.size()) { m_sel=idx; Refresh(); } }
private:
  int TabWidth(wxDC &dc, int i) { int w=dc.GetTextExtent(m_tabs[i].label).x+24; if (m_tabs[i].closable) w+=16; return std::max(w,70); }
  int TabAtX(int x) { wxClientDC dc(this); dc.SetFont(GetFont().Scaled(0.9f)); int cx=0; for (int i=0;i<(int)m_tabs.size();++i) { int w=TabWidth(dc,i); if (x>=cx&&x<cx+w) return i; cx+=w; } return -1; }
  int CloseAtXY(int x, int y) { wxClientDC dc(this); dc.SetFont(GetFont().Scaled(0.9f)); int cx=0; for (int i=0;i<(int)m_tabs.size();++i) { int w=TabWidth(dc,i); if (m_tabs[i].closable&&x>=cx+w-18&&x<cx+w-4&&y>=4&&y<m_tabH-4) return i; cx+=w; } return -1; }
  void OnClick(wxMouseEvent &e) {
    int ci=CloseAtXY(e.GetX(),e.GetY()); if (ci>=0&&onClose) { onClose(ci); return; }
    int idx=TabAtX(e.GetX()); if (idx>=0&&idx!=m_sel) { m_sel=idx; Refresh(); if (onSelect) onSelect(idx); }
  }
  void OnPaint(wxPaintEvent&) {
    wxAutoBufferedPaintDC dc(this); wxRect cr=GetClientRect();
    dc.SetBrush(wxBrush(m_bg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(cr);
    dc.SetPen(wxPen(m_border)); dc.DrawLine(0,cr.height-1,cr.width,cr.height-1);
    wxFont f=GetFont().Scaled(0.9f); dc.SetFont(f);
    int cx=0;
    for (int i=0;i<(int)m_tabs.size();++i) {
      int w=TabWidth(dc,i); bool sel=(i==m_sel), hov=(i==m_hov&&!sel);
      if (sel) {
        dc.SetBrush(wxBrush(m_selBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(cx,0,w,cr.height+1);
        dc.SetBrush(wxBrush(m_accentBg)); dc.DrawRectangle(cx,0,w,2);
        dc.SetPen(wxPen(m_border)); dc.DrawLine(cx,0,cx,cr.height); dc.DrawLine(cx+w-1,0,cx+w-1,cr.height);
      } else if (hov) {
        dc.SetBrush(wxBrush(m_selBg.ChangeLightness(96))); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(cx,0,w,cr.height);
      }
      dc.SetTextForeground(sel?m_selFg:m_fg);
      dc.DrawText(m_tabs[i].label, cx+12, (cr.height-dc.GetCharHeight())/2);
      if (m_tabs[i].closable) {
        int bx=cx+w-17, by=(cr.height-8)/2;
        wxColour cc=(m_hovClose==i)?wxColour(220,50,40):(sel?m_selFg.ChangeLightness(70):m_fg.ChangeLightness(70));
        dc.SetPen(wxPen(cc,1)); dc.DrawLine(bx,by,bx+7,by+7); dc.DrawLine(bx+7,by,bx,by+7);
      }
      cx+=w;
    }
  }
};

// ===== Wazoo =====
class Wazoo : public wxWindow {
public:
  struct TreeItem { wxString name, icon; int count, depth; bool isSep, expanded, collapsed; };
  std::vector<TreeItem> m_tree;
  int m_treeSel=0, m_treeHov=-1, m_treeItemH=26;
  struct WTab { wxString label; };
  std::vector<WTab> m_tabs;
  int m_tabSel=0, m_tabHov=-1, m_tabH=30;
  wxColour m_bg, m_fg, m_selBg, m_selFg, m_hoverBg, m_dim, m_line;
  wxColour m_tabBg, m_tabFg, m_tabSelBg, m_tabSelFg, m_countBg, m_countFg;
  std::function<void(int)> onMailboxSelect;

  Wazoo(wxWindow *p) : wxWindow(p,wxID_ANY) {
    m_tree = {
      {"In",            "\xF0\x9F\x93\xA5", 3, 0, false, false, false},
      {"Out",           "\xF0\x9F\x93\xA4", 0, 0, false, false, false},
      {"Drafts",        "\xE2\x9C\x8F",     1, 0, false, false, false},
      {"Trash",         "\xF0\x9F\x97\x91", 0, 0, false, false, false},
      {"Junk",          "\xE2\x9B\x94",     5, 0, false, false, false},
      {"",              "", 0, 0, true, false, false},
      {"Work",          "\xF0\x9F\x92\xBC", 0, 0, false, true, false},
      {"Projects",      "\xF0\x9F\x93\x81", 0, 1, false, true, false},
      {"Active",        "\xF0\x9F\x94\xB5", 2, 2, false, false, false},
      {"Completed",     "\xE2\x9C\x85",     0, 2, false, false, false},
      {"Reviews",       "\xF0\x9F\x94\x8D", 3, 1, false, false, false},
      {"Archive",       "\xF0\x9F\x93\xA6", 0, 1, false, true, false},
      {"2025",          "\xF0\x9F\x93\x85", 0, 2, false, false, false},
      {"2024",          "\xF0\x9F\x93\x85", 0, 2, false, false, false},
      {"Personal",      "\xF0\x9F\x8F\xA0", 0, 0, false, true, false},
      {"Travel",        "\xE2\x9C\x88",     0, 1, false, false, false},
      {"Receipts",      "\xF0\x9F\xA7\xBE", 0, 1, false, false, false},
      {"Photos",        "\xF0\x9F\x93\xB8", 2, 1, false, false, false},
      {"Mailing Lists", "\xF0\x9F\x93\x8B", 12, 0, false, true, false},
      {"Eudora-Dev",    "\xE2\x9A\x99",     4, 1, false, false, false},
      {"Tech-Talk",     "\xF0\x9F\x92\xAC", 8, 1, false, false, false},
      {"Rust-Users",    "\xF0\x9F\xA6\x80", 0, 1, false, false, false},
    };
    m_tabs = {{"Mailboxes"}, {"Signatures"}, {"Stationery"}};
    SetBackgroundStyle(wxBG_STYLE_PAINT); SetMinSize(wxSize(200,-1)); SetCursor(wxCURSOR_HAND);
    Bind(wxEVT_PAINT, &Wazoo::OnPaint, this);
    Bind(wxEVT_LEFT_DOWN, &Wazoo::OnClick, this);
    Bind(wxEVT_MOTION, [this](wxMouseEvent &e) {
      wxRect cr=GetClientRect();
      int ti=-1; if (e.GetY()>=cr.height-m_tabH) { int tw=cr.width/(int)m_tabs.size(); ti=std::min(e.GetX()/tw,(int)m_tabs.size()-1); }
      int hovIdx=-1;
      if (e.GetY()<cr.height-m_tabH && m_tabSel==0) {
        auto vis=VisibleItems();
        int row=e.GetY()/m_treeItemH;
        if (row>=0&&row<(int)vis.size()) hovIdx=vis[row];
      }
      if (hovIdx!=m_treeHov||ti!=m_tabHov) { m_treeHov=hovIdx; m_tabHov=ti; Refresh(); }
    });
    Bind(wxEVT_LEAVE_WINDOW, [this](wxMouseEvent&) { m_treeHov=-1; m_tabHov=-1; Refresh(); });
  }
  void SetTheme(const Theme &t) {
    m_bg=t.treeBg; m_fg=t.treeFg; m_selBg=t.treeSelBg; m_selFg=t.treeSelFg;
    m_hoverBg=t.treeHoverBg; m_dim=t.treeDim; m_line=t.treeLine;
    m_tabBg=t.wazooTabBg; m_tabFg=t.wazooTabFg; m_tabSelBg=t.wazooTabSelBg; m_tabSelFg=t.wazooTabSelFg;
    m_countBg=t.accentBg; m_countFg=t.accentFg; Refresh();
  }
private:
  // Get visible items (skip children of collapsed parents)
  std::vector<int> VisibleItems() {
    std::vector<int> vis;
    for (int i=0;i<(int)m_tree.size();++i) {
      // Check if any ancestor is collapsed
      bool hidden=false;
      if (m_tree[i].depth>0) {
        for (int j=i-1;j>=0;--j) {
          if (m_tree[j].depth<m_tree[i].depth && m_tree[j].expanded && m_tree[j].collapsed) { hidden=true; break; }
          if (m_tree[j].depth<m_tree[i].depth && !m_tree[j].expanded) break;
        }
      }
      if (!hidden) vis.push_back(i);
    }
    return vis;
  }

  void OnClick(wxMouseEvent &e) {
    wxRect cr=GetClientRect();
    if (e.GetY()>=cr.height-m_tabH) {
      int tw=cr.width/(int)m_tabs.size(); int idx=std::min(e.GetX()/tw,(int)m_tabs.size()-1);
      if (idx!=m_tabSel) { m_tabSel=idx; m_treeSel=0; Refresh(); } return;
    }
    if (m_tabSel==0) {
      auto vis=VisibleItems();
      int row=e.GetY()/m_treeItemH;
      if (row>=0&&row<(int)vis.size()) {
        int idx=vis[row];
        if (m_tree[idx].isSep) return;
        // Toggle collapse if it's a parent
        if (m_tree[idx].expanded) {
          m_tree[idx].collapsed=!m_tree[idx].collapsed;
        }
        m_treeSel=idx; Refresh();
        if (onMailboxSelect) onMailboxSelect(idx);
      }
    }
  }
  void OnPaint(wxPaintEvent&) {
    wxAutoBufferedPaintDC dc(this); wxRect cr=GetClientRect();
    dc.SetBrush(wxBrush(m_bg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(cr);
    wxFont nf=GetFont(), sf=nf.Scaled(0.82f), cf=nf.Scaled(0.78f), bf=nf.Bold();
    int bottom=cr.height-m_tabH;

    if (m_tabSel==0) {
      auto vis=VisibleItems();
      for (int row=0;row<(int)vis.size();++row) {
        int i=vis[row];
        int y=row*m_treeItemH; if (y>=bottom) break;
        const auto &it=m_tree[i];
        if (it.isSep) { dc.SetPen(wxPen(m_line)); dc.DrawLine(10,y+m_treeItemH/2,cr.width-10,y+m_treeItemH/2); continue; }
        bool sel=(i==m_treeSel); int indent=it.depth*18;
        if (sel) { dc.SetBrush(wxBrush(m_selBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRoundedRectangle(4,y+2,cr.width-8,m_treeItemH-4,5); }
        else if (i==m_treeHov) { dc.SetBrush(wxBrush(m_hoverBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRoundedRectangle(4,y+2,cr.width-8,m_treeItemH-4,5); }
        dc.SetFont(nf); dc.SetTextForeground(sel?m_selFg:m_fg);
        int x=12+indent, ty=y+(m_treeItemH-dc.GetCharHeight())/2;
        if (it.expanded) {
          dc.SetFont(sf); dc.SetTextForeground(sel?m_selFg:m_dim);
          dc.DrawText(it.collapsed?"\xE2\x96\xB6":"\xE2\x96\xBC",x-2,ty+1);
          dc.SetFont(nf); dc.SetTextForeground(sel?m_selFg:m_fg);
        }
        if (!it.icon.empty()) { dc.DrawText(it.icon,x+12,ty); x+=dc.GetTextExtent(it.icon).x+18; } else x+=12;
        dc.DrawText(it.name,x,ty);
        if (it.count>0) {
          dc.SetFont(cf); wxString cnt=wxString::Format("%d",it.count); wxSize cs=dc.GetTextExtent(cnt);
          int bw=std::max(cs.x+10,20), bx=cr.width-bw-10, by=y+(m_treeItemH-16)/2;
          dc.SetBrush(wxBrush(sel?m_selFg:m_countBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRoundedRectangle(bx,by,bw,16,8);
          dc.SetTextForeground(sel?m_selBg:m_countFg); dc.DrawText(cnt,bx+(bw-cs.x)/2,by+(16-cs.y)/2); dc.SetFont(nf);
        }
      }
    } else {
      wxString title = m_tabSel==1 ? "Signatures" : "Stationery";
      dc.SetFont(bf); dc.SetTextForeground(m_fg); dc.DrawText(title,12,8);
      dc.SetPen(wxPen(m_line)); dc.DrawLine(10,30,cr.width-10,30);
      wxString items1[] = {"Standard","Professional","Informal","Auto-Reply"};
      wxString items2[] = {"Plain Text","Business Letter","Newsletter","Meeting Invite","Bug Report"};
      wxString *items = m_tabSel==1 ? items1 : items2;
      int count = m_tabSel==1 ? 4 : 5;
      for (int i=0;i<count;++i) {
        int y=34+i*m_treeItemH; if (y>=bottom) break;
        bool sel=(i==m_treeSel);
        if (sel) { dc.SetBrush(wxBrush(m_selBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRoundedRectangle(4,y+2,cr.width-8,m_treeItemH-4,5); }
        else if (i==m_treeHov) { dc.SetBrush(wxBrush(m_hoverBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRoundedRectangle(4,y+2,cr.width-8,m_treeItemH-4,5); }
        dc.SetFont(nf); dc.SetTextForeground(sel?m_selFg:m_fg);
        wxString ico = m_tabSel==1?"\xE2\x9C\x8D":"\xF0\x9F\x93\x84";
        dc.DrawText(ico+" "+items[i],16,y+(m_treeItemH-dc.GetCharHeight())/2);
      }
    }

    // Bottom tabs
    dc.SetBrush(wxBrush(m_tabBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(0,bottom,cr.width,m_tabH);
    dc.SetPen(wxPen(m_line)); dc.DrawLine(0,bottom,cr.width,bottom);
    int nt=(int)m_tabs.size(), tw=cr.width/nt; dc.SetFont(sf);
    for (int i=0;i<nt;++i) {
      int tx=i*tw, tabW=(i==nt-1)?(cr.width-tx):tw;
      bool tsel=(i==m_tabSel), thov=(i==m_tabHov&&!tsel);
      if (tsel) { dc.SetBrush(wxBrush(m_tabSelBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(tx,bottom+1,tabW,m_tabH-1);
        dc.SetBrush(wxBrush(m_countBg)); dc.DrawRectangle(tx,bottom,tabW,2);
      } else if (thov) { dc.SetBrush(wxBrush(m_hoverBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(tx,bottom+1,tabW,m_tabH-1); }
      dc.SetTextForeground(tsel?m_tabSelFg:m_tabFg); wxSize ts=dc.GetTextExtent(m_tabs[i].label);
      dc.DrawText(m_tabs[i].label,tx+(tabW-ts.x)/2,bottom+(m_tabH-ts.y)/2);
      if (i>0) { dc.SetPen(wxPen(m_line)); dc.DrawLine(tx,bottom+6,tx,bottom+m_tabH-6); }
    }
  }
};

// ===== Email data =====
enum Priority { PRI_NONE, PRI_LOW, PRI_NORMAL, PRI_HIGH, PRI_HIGHEST };
struct Email {
  const char *from, *to, *cc, *subject, *date, *size, *body;
  bool unread, replied, forwarded, hasAttach;
  Priority pri;
  bool flagged;
};

static Email kInbox[] = {
  {"Steve Dorner <sdorner@qualcomm.com>", "you@example.com", "",
   "Welcome to Eudora 2026!", "Mar 7, 2026  10:15 AM", "4K",
   "<p>Welcome to the new Eudora!</p>"
   "<p>We've rebuilt Eudora from the ground up for 2026, keeping the spirit "
   "of the original while adding modern features.</p>"
   "<ul><li>Rich HTML composition with live preview</li>"
   "<li>Full dark mode support</li><li>Blazing fast search</li>"
   "<li>Privacy-first &mdash; no tracking, no ads</li></ul>"
   "<p>Best regards,<br/>Steve</p>",
   true, false, false, false, PRI_HIGHEST, true},

  {"Jeff Beckley <jbeckley@qualcomm.com>", "eudora-dev@list.example.com", "sdorner@qualcomm.com; dpark@example.com",
   "Re: Re: Re: Re: Re: Toolbar redesign proposal", "Mar 7, 2026  9:42 AM", "12K",
   // 5 levels of Eudora-style colored quoting - a threaded discussion
   "<p>Looks like we have consensus then. Ship it!</p><p>Jeff</p>"

   "<blockquote style=\"margin: 12px 0 0 0; padding: 0 0 0 10px; border-left: 3px solid #326ea5;\">"
   "<p style=\"font-size: 9pt; opacity: 0.5;\">At Mar 7, 8:30 AM, Dana Park wrote:</p>"
   "<p>I agree with Steve. The separator lines really help with visual grouping. "
   "One small nit: can we make the hover state a bit more subtle?</p>"

   "<blockquote style=\"margin: 12px 0 0 0; padding: 0 0 0 10px; border-left: 3px solid #d23720;\">"
   "<p style=\"font-size: 9pt; opacity: 0.5;\">At Mar 6, 4:15 PM, Steve Dorner wrote:</p>"
   "<p>The spacing is perfect on macOS. I tested on both Retina and non-Retina "
   "displays. The icon-to-text gap at 5px feels natural.</p>"
   "<p>Regarding keyboard shortcuts in tooltips &mdash; yes, absolutely. "
   "This was one of the most-requested features from the beta testers.</p>"

   "<blockquote style=\"margin: 12px 0 0 0; padding: 0 0 0 10px; border-left: 3px solid #2ea043;\">"
   "<p style=\"font-size: 9pt; opacity: 0.5;\">At Mar 6, 2:00 PM, Jeff Beckley wrote:</p>"
   "<p>Here's my assessment of the toolbar redesign:</p>"
   "<ol><li>Icon spacing feels right on macOS</li>"
   "<li>Should we add keyboard shortcuts to tooltips?</li>"
   "<li>Separator lines between groups look clean</li></ol>"

   "<blockquote style=\"margin: 12px 0 0 0; padding: 0 0 0 10px; border-left: 3px solid #8b5cf6;\">"
   "<p style=\"font-size: 9pt; opacity: 0.5;\">At Mar 6, 11:00 AM, Lisa Warren wrote:</p>"
   "<p>From a UX perspective, the new toolbar is a big improvement. "
   "The grouping of related actions (Reply/Reply All/Forward) matches "
   "user mental models well.</p>"
   "<p>I ran a quick usability test with 5 participants &mdash; all found "
   "the new layout more intuitive than the previous version.</p>"

   "<blockquote style=\"margin: 12px 0 0 0; padding: 0 0 0 10px; border-left: 3px solid #d97706;\">"
   "<p style=\"font-size: 9pt; opacity: 0.5;\">At Mar 5, 3:45 PM, Bob Chen wrote:</p>"
   "<p>Initial toolbar mockup attached. Key design decisions:</p>"
   "<ul><li>Flat buttons with hover highlight (no borders)</li>"
   "<li>Emoji icons for universal rendering</li>"
   "<li>Accent color on primary action (New Message)</li>"
   "<li>Vertical separators between logical groups</li></ul>"
   "<p>Let me know what you all think.</p>"
   "</blockquote>"
   "</blockquote>"
   "</blockquote>"
   "</blockquote>"
   "</blockquote>",
   true, false, false, false, PRI_NORMAL, false},

  {"Dana Park <dpark@example.com>", "you@example.com", "lwarren@design.example",
   "\xF0\x9F\x93\x85 Calendar: Design Review Meeting", "Mar 7, 2026  8:00 AM", "6K",
   "<div style=\"border: 1px solid #ccc; border-radius: 10px; overflow: hidden; margin-bottom: 16px;\">"
   "<div style=\"background: #326ea5; padding: 16px 20px; color: white;\">"
   "<div style=\"font-size: 11pt; opacity: 0.8;\">CALENDAR INVITATION</div>"
   "<div style=\"font-size: 16pt; font-weight: bold; margin-top: 4px;\">\xF0\x9F\x93\x85 Design Review Meeting</div></div>"
   "<div style=\"padding: 16px 20px;\">"
   "<table style=\"width: 100%; border-collapse: collapse;\">"
   "<tr><td style=\"padding: 8px 0; width: 95px; opacity: 0.5; font-size: 9pt; text-transform: uppercase; font-weight: bold; vertical-align: top;\">When</td>"
   "<td style=\"padding: 8px 0;\"><b>Friday, March 14, 2026</b><br/>"
   "<span style=\"opacity: 0.7;\">2:00 PM \xE2\x80\x93 3:30 PM (PST) \xE2\x80\xA2 1h 30m</span></td></tr>"
   "<tr><td style=\"padding: 8px 0; opacity: 0.5; font-size: 9pt; text-transform: uppercase; font-weight: bold; vertical-align: top;\">Where</td>"
   "<td style=\"padding: 8px 0;\">\xF0\x9F\x93\x8D Conference Room B<br/>"
   "<span style=\"opacity: 0.7;\">\xF0\x9F\x96\xA5 Zoom: zoom.us/j/123456789</span></td></tr>"
   "<tr><td style=\"padding: 8px 0; opacity: 0.5; font-size: 9pt; text-transform: uppercase; font-weight: bold;\">Organizer</td>"
   "<td style=\"padding: 8px 0;\">Dana Park</td></tr>"
   "<tr><td style=\"padding: 8px 0; opacity: 0.5; font-size: 9pt; text-transform: uppercase; font-weight: bold; vertical-align: top;\">Attendees</td>"
   "<td style=\"padding: 8px 0;\">"
   "\xE2\x9C\x85 You &nbsp; \xE2\x80\xA2 &nbsp; \xE2\x9D\x93 Lisa Warren &nbsp; \xE2\x80\xA2 &nbsp; \xE2\x9D\x93 Bob Chen &nbsp; \xE2\x80\xA2 &nbsp; \xE2\x9D\x93 Maria Santos"
   "</td></tr></table>"
   "<div style=\"margin: 14px 0; padding: 12px; background: rgba(0,0,0,0.04); border-radius: 6px;\">"
   "<div style=\"font-size: 9pt; text-transform: uppercase; font-weight: bold; opacity: 0.5; margin-bottom: 6px;\">Agenda</div>"
   "<ol style=\"margin: 0; padding-left: 18px;\">"
   "<li>Dashboard wireframe review</li><li>Mobile nav patterns</li>"
   "<li>Color palette finalization</li><li>Q&amp;A / next steps</li></ol></div>"
   "<table style=\"width: 100%; border-collapse: collapse; margin-top: 12px;\"><tr>"
   "<td style=\"width: 33%; text-align: center; padding: 10px;\">"
   "<div style=\"background: #2ea043; color: white; border-radius: 6px; padding: 8px 0; font-weight: bold;\">"
   "\xE2\x9C\x93 Accept</div></td>"
   "<td style=\"width: 33%; text-align: center; padding: 10px;\">"
   "<div style=\"background: #d97706; color: white; border-radius: 6px; padding: 8px 0; font-weight: bold;\">"
   "\xE2\x9C\xBF Tentative</div></td>"
   "<td style=\"width: 33%; text-align: center; padding: 10px;\">"
   "<div style=\"border: 1px solid #ccc; border-radius: 6px; padding: 8px 0; font-weight: bold; opacity: 0.6;\">"
   "\xE2\x9C\x97 Decline</div></td>"
   "</tr></table></div></div>"
   "<p>Hi all, let's sync on the dashboard redesign. Please review the wireframes beforehand.</p>"
   "<p>Dana</p>",
   true, false, false, true, PRI_HIGH, true},

  {"Alan Strider <astrider@example.com>", "team@example.com", "",
   "Attachment: Q1 Budget Report", "Mar 6, 2026  4:18 PM", "842K",
   "<p>Hi team,</p><p>Q1 budget report attached. Highlights:</p>"
   "<ul><li>Engineering 3% under budget</li><li>Marketing up 7% (planned)</li>"
   "<li>Server costs down 12% post-migration</li></ul>"
   "<p>Review by Thursday please.</p><p>Alan</p>",
   false, false, false, true, PRI_NORMAL, false},

  {"Mailing List <announce@eudora.example>", "users@eudora.example", "",
   "[Announce] Eudora 2026 Beta 3 available", "Mar 6, 2026  2:30 PM", "2K",
   "<p><b>Eudora 2026 Beta 3</b> is now available.</p>"
   "<ul><li>Fixed crash on large attachments</li><li>Improved IMAP IDLE stability</li>"
   "<li>Drag-and-drop mailbox reordering</li><li>Dark mode fixes on Linux</li></ul>",
   false, true, false, false, PRI_NONE, false},

  {"Security Alert <noreply@auth.example>", "you@example.com", "",
   "New sign-in from Chrome on macOS", "Mar 3, 2026  7:30 PM", "2K",
   "<p>New sign-in detected:</p>"
   "<table style=\"width: 100%; border-collapse: collapse; margin: 12px 0;\">"
   "<tr><td style=\"padding: 4px 12px;\">Device</td><td style=\"padding: 4px 12px;\">Chrome on macOS</td></tr>"
   "<tr><td style=\"padding: 4px 12px;\">Location</td><td style=\"padding: 4px 12px;\">San Francisco, CA</td></tr>"
   "<tr><td style=\"padding: 4px 12px;\">Time</td><td style=\"padding: 4px 12px;\">Mar 3, 7:30 PM PST</td></tr></table>"
   "<p>If this wasn't you, secure your account immediately.</p>",
   false, false, false, false, PRI_HIGH, false},

  {"Newsletter <digest@techweekly.example>", "subscribers@techweekly.example", "",
   "This Week in Tech: AI, Open Source, and More", "Mar 3, 2026  6:00 AM", "8K",
   "<p><b>This Week in Tech</b> &mdash; March 3, 2026</p><h3>Top Stories</h3>"
   "<ol><li>Open source email clients making a comeback</li>"
   "<li>New CSS features in all major browsers</li><li>Rust adoption continues to grow</li></ol>",
   false, false, false, false, PRI_NONE, false},

  {"Rick Langley <rick@example.com>", "you@example.com", "",
   "Lunch Friday?", "Mar 2, 2026  1:12 PM", "1K",
   "<p>Hey, want to grab lunch Friday? Thinking that new ramen place on 5th.</p>"
   "<p>Let me know!</p><p>Rick</p>",
   false, false, false, false, PRI_NONE, false},
};
static const int kNumInbox = sizeof(kInbox)/sizeof(kInbox[0]);

static Email kOutbox[] = {
  {"you@example.com", "Steve Dorner <sdorner@qualcomm.com>", "",
   "Re: Welcome to Eudora 2026!", "Mar 7, 2026  10:45 AM", "3K",
   "<p>Thanks Steve! The new version is amazing.</p><p>Quick questions:</p>"
   "<ol><li>Can I import old mailboxes?</li><li>CalDAV support?</li></ol>",
   false, false, false, false, PRI_NORMAL, false},
  {"you@example.com", "team@example.com", "astrider@example.com",
   "Dashboard redesign - timeline update", "Mar 6, 2026  3:00 PM", "2K",
   "<p>Hi team,</p><p>Running behind on the dashboard redesign. Pushing milestone to next Friday.</p>",
   false, false, false, false, PRI_NORMAL, false},
  {"you@example.com", "Dana Park <dpark@example.com>", "",
   "Re: Conference talk proposal", "Mar 5, 2026  2:15 PM", "2K",
   "<p>Fantastic news, Dana!</p><p>Outline ideas:</p>"
   "<ol><li>History of desktop email clients</li><li>Modern rendering challenges</li><li>Live demo</li></ol>",
   false, false, false, false, PRI_NONE, false},
  {"you@example.com", "Rick Langley <rick@example.com>", "",
   "Re: Lunch Friday?", "Mar 2, 2026  1:30 PM", "1K",
   "<p>Sounds great! 12:30 at the ramen place works.</p>",
   false, false, false, false, PRI_NONE, false},
};
static const int kNumOutbox = sizeof(kOutbox)/sizeof(kOutbox[0]);

// ===== Contact data =====
struct Contact { const char *name, *email, *phone, *company, *title, *notes; wxColour avatarColor; };
static Contact kContacts[] = {
  {"Steve Dorner",    "sdorner@qualcomm.com",    "(858) 555-0100", "Qualcomm",   "Chief Architect",     "Creator of Eudora. Prefers plain text.", wxColour(50,110,165)},
  {"Jeff Beckley",    "jbeckley@qualcomm.com",   "(858) 555-0101", "Qualcomm",   "Senior Engineer",     "Toolbar and UI specialist.",             wxColour(165,85,50)},
  {"Alan Strider",    "astrider@example.com",    "(415) 555-0200", "Acme Corp",  "Finance Director",    "Sends quarterly reports.",               wxColour(46,160,67)},
  {"Dana Park",       "dpark@example.com",       "(415) 555-0201", "TechStudio", "Design Lead",         "Conference co-speaker.",                 wxColour(155,50,165)},
  {"Rick Langley",    "rick@example.com",        "(510) 555-0300", "",           "",                    "College friend. Likes ramen.",           wxColour(210,153,34)},
  {"Lisa Warren",     "lwarren@design.example",  "(510) 555-0301", "DesignWorks","UX Researcher",       "Working on mobile nav project.",         wxColour(210,55,45)},
  {"Bob Chen",        "bchen@example.com",       "(650) 555-0400", "CloudSoft",  "DevOps Manager",      "Handles infrastructure.",               wxColour(50,165,130)},
  {"Maria Santos",    "msantos@example.com",     "(650) 555-0401", "DataFlow",   "Data Scientist",      "ML and analytics expert.",              wxColour(100,80,180)},
};
static const int kNumContacts = sizeof(kContacts)/sizeof(kContacts[0]);

// ===== Filter data =====
struct Filter { const char *name, *match, *action; bool enabled; };
static Filter kFilters[] = {
  {"Mailing Lists",   "To contains @list.example",     "Move to Mailing Lists",  true},
  {"Newsletters",     "From contains newsletter",      "Move to Newsletters",    true},
  {"Spam Keywords",   "Subject contains winner|lottery","Move to Junk",           true},
  {"Work Priority",   "From contains @qualcomm.com",   "Set Priority: High, Flag", true},
  {"Large Attach",    "Attachment Size > 5MB",          "Add Label: Large",       false},
  {"Auto-Reply OOO",  "Subject is Out of Office",      "Skip Inbox, Auto-Reply", true},
  {"Conference",      "Subject contains conference|summit", "Move to Work/Projects", false},
};
static const int kNumFilters = sizeof(kFilters)/sizeof(kFilters[0]);

// ===== Message list =====
class MsgList : public wxWindow {
public:
  wxColour m_bg, m_fg, m_dim, m_selBg, m_selFg, m_hoverBg, m_hdrBg, m_hdrFg, m_grid, m_unread, m_stripe;
  int m_sel=0, m_hov=-1, m_rowH=22, m_hdrH=24;
  Email *m_data=nullptr; int m_count=0; bool m_isOut=false;
  std::function<void(int)> onSelect;
  int cS=24, cP=22, cA=22, cF=22, cWho=185, cDate=150, cSz=55;

  MsgList(wxWindow *p) : wxWindow(p,wxID_ANY) {
    SetBackgroundStyle(wxBG_STYLE_PAINT); SetCursor(wxCURSOR_HAND);
    Bind(wxEVT_PAINT, &MsgList::OnPaint, this);
    Bind(wxEVT_LEFT_DOWN, [this](wxMouseEvent &e) {
      int idx=(e.GetY()-m_hdrH)/m_rowH;
      if (idx>=0&&idx<m_count&&idx!=m_sel) { m_sel=idx; Refresh(); if (onSelect) onSelect(idx); }
    });
    Bind(wxEVT_MOTION, [this](wxMouseEvent &e) { int idx=(e.GetY()-m_hdrH)/m_rowH; if (idx!=m_hov) { m_hov=idx; Refresh(); } });
    Bind(wxEVT_LEAVE_WINDOW, [this](wxMouseEvent&) { m_hov=-1; Refresh(); });
  }
  void SetData(Email *d, int n, bool out) { m_data=d; m_count=n; m_isOut=out; m_sel=0; Refresh(); }
  void SetTheme(const Theme &t) {
    m_bg=t.listBg; m_fg=t.listFg; m_dim=t.listDim; m_selBg=t.listSelBg; m_selFg=t.listSelFg;
    m_hoverBg=t.listHoverBg; m_hdrBg=t.listHeaderBg; m_hdrFg=t.listHeaderFg;
    m_grid=t.listGrid; m_unread=t.listUnread; m_stripe=t.listStripe; Refresh();
  }
private:
  void OnPaint(wxPaintEvent&) {
    wxAutoBufferedPaintDC dc(this); wxRect cr=GetClientRect();
    dc.SetBrush(wxBrush(m_bg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(cr);
    if (!m_data) return;
    wxFont nf=GetFont(), bf=nf.Bold(), sf=nf.Scaled(0.88f), tf=nf.Scaled(0.78f);
    int fixW=cS+cP+cA+cF+cWho+cDate+cSz, cSubj=std::max(cr.width-fixW,80);
    dc.SetBrush(wxBrush(m_hdrBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(0,0,cr.width,m_hdrH);
    dc.SetFont(sf); dc.SetTextForeground(m_hdrFg); int hy=(m_hdrH-dc.GetCharHeight())/2, hx=cS+cP+cA+cF;
    dc.DrawText(m_isOut?"To":"Who",hx+6,hy); hx+=cWho; dc.DrawText("Date",hx+6,hy); hx+=cDate;
    dc.DrawText("Size",hx+4,hy); hx+=cSz; dc.DrawText("Subject",hx+6,hy);
    dc.SetPen(wxPen(m_grid)); dc.DrawLine(0,m_hdrH-1,cr.width,m_hdrH-1);
    for (int i=0;i<m_count;++i) {
      int y=m_hdrH+i*m_rowH; if (y>cr.height) break;
      const auto &e=m_data[i]; bool sel=(i==m_sel);
      if (sel) { dc.SetBrush(wxBrush(m_selBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(0,y,cr.width,m_rowH); }
      else if (i==m_hov) { dc.SetBrush(wxBrush(m_hoverBg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(0,y,cr.width,m_rowH); }
      else if (i%2==1) { dc.SetBrush(wxBrush(m_stripe)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(0,y,cr.width,m_rowH); }
      wxColour fg=sel?m_selFg:(e.unread?m_unread:m_fg), dim=sel?m_selFg:m_dim;
      int x=0, ty=y+(m_rowH-dc.GetCharHeight())/2;
      dc.SetFont(tf);
      if (e.unread) { dc.SetBrush(wxBrush(wxColour(50,110,165))); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawCircle(x+cS/2,y+m_rowH/2,3); }
      else if (e.replied) { dc.SetTextForeground(dim); dc.DrawText("\xE2\x86\xA9",x+6,ty); }
      else if (e.forwarded) { dc.SetTextForeground(dim); dc.DrawText("\xE2\x86\x92",x+6,ty); }
      x+=cS;
      if (e.pri==PRI_HIGHEST) { dc.SetTextForeground(wxColour(210,55,45)); dc.DrawText("\xE2\x96\xB2\xE2\x96\xB2",x+1,ty); }
      else if (e.pri==PRI_HIGH) { dc.SetTextForeground(wxColour(210,55,45)); dc.DrawText("\xE2\x96\xB2",x+5,ty); }
      x+=cP; if (e.hasAttach) { dc.SetTextForeground(dim); dc.DrawText("\xF0\x9F\x93\x8E",x+2,ty); }
      x+=cA; if (e.flagged) { dc.SetTextForeground(wxColour(210,55,45)); dc.DrawText("\xE2\x9A\x91",x+2,ty); }
      x+=cF; dc.SetFont(e.unread?bf:nf); dc.SetTextForeground(fg);
      wxString who=m_isOut?e.to:e.from; int angle=who.Find('<'); wxString disp=(angle!=wxNOT_FOUND)?who.Left(angle).Trim():who;
      dc.SetClippingRegion(x,y,cWho-4,m_rowH); dc.DrawText(disp,x+6,ty); dc.DestroyClippingRegion(); x+=cWho;
      dc.SetFont(nf); dc.SetTextForeground(dim);
      dc.SetClippingRegion(x,y,cDate-4,m_rowH); dc.DrawText(e.date,x+6,ty); dc.DestroyClippingRegion(); x+=cDate;
      dc.SetFont(sf); wxSize szE=dc.GetTextExtent(e.size); dc.DrawText(e.size,x+cSz-szE.x-6,ty); x+=cSz;
      dc.SetFont(e.unread?bf:nf); dc.SetTextForeground(fg);
      dc.SetClippingRegion(x,y,cSubj-4,m_rowH); dc.DrawText(e.subject,x+6,ty); dc.DestroyClippingRegion();
      dc.SetPen(wxPen(m_grid)); dc.DrawLine(0,y+m_rowH-1,cr.width,y+m_rowH-1);
    }
    dc.SetPen(wxPen(m_grid)); int cx=0;
    for (int w:{cS,cP,cA,cF,cWho,cDate,cSz}) { cx+=w; dc.DrawLine(cx,0,cx,std::min(m_hdrH+m_count*m_rowH,cr.height)); }
  }
};

// ===== Message header bar =====
class MsgHeader : public wxWindow {
public:
  wxColour m_bg, m_fg, m_dim, m_border;
  Email *m_email=nullptr;
  MsgHeader(wxWindow *p) : wxWindow(p,wxID_ANY) {
    SetBackgroundStyle(wxBG_STYLE_PAINT); SetMinSize(wxSize(-1,74));
    Bind(wxEVT_PAINT, &MsgHeader::OnPaint, this);
  }
  void SetEmail(Email *e) { m_email=e; Refresh(); }
  void SetTheme(const Theme &t) { m_bg=t.headerBg; m_fg=t.headerFg; m_dim=t.headerDim; m_border=t.headerBorder; Refresh(); }
private:
  void OnPaint(wxPaintEvent&) {
    wxAutoBufferedPaintDC dc(this); wxRect r=GetClientRect();
    dc.SetBrush(wxBrush(m_bg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(r);
    dc.SetPen(wxPen(m_border)); dc.DrawLine(0,r.height-1,r.width,r.height-1);
    if (!m_email) return;
    wxFont nf=GetFont(), bf=nf.Bold(), sf=nf.Scaled(0.88f);
    int y=7, lw=55;
    dc.SetFont(sf); dc.SetTextForeground(m_dim); dc.DrawText("From:",10,y);
    dc.SetTextForeground(m_fg); dc.DrawText(m_email->from,lw+2,y); y+=16;
    dc.SetTextForeground(m_dim); dc.DrawText("To:",10,y);
    dc.SetTextForeground(m_fg); dc.DrawText(m_email->to,lw+2,y);
    if (strlen(m_email->cc)>0) {
      int tw2=dc.GetTextExtent(wxString(m_email->to)).x;
      dc.SetTextForeground(m_dim); dc.DrawText("  Cc:",lw+tw2+6,y);
      dc.SetTextForeground(m_fg); dc.DrawText(m_email->cc,lw+tw2+36,y);
    } y+=16;
    dc.SetTextForeground(m_dim); dc.DrawText("Subj:",10,y);
    dc.SetFont(bf); dc.SetTextForeground(m_fg); dc.DrawText(m_email->subject,lw+2,y); y+=16;
    dc.SetFont(sf); dc.SetTextForeground(m_dim);
    wxString info=wxString::Format("Date: %s    Size: %s",m_email->date,m_email->size);
    if (m_email->pri>=PRI_HIGH) info+="    Priority: HIGH";
    if (m_email->hasAttach) info+="    \xF0\x9F\x93\x8E Attachment";
    dc.DrawText(info,10,y);
  }
};

// ===== Compose header =====
class ComposeHeader : public wxWindow {
public:
  wxColour m_bg, m_fg, m_dim, m_border, m_inputBg, m_inputFg, m_labelFg, m_accent;
  wxString m_from="you@example.com  <Eudora User>", m_to, m_cc, m_bcc, m_subject, m_attach, m_priority="Normal";
  int m_rowH=24, m_labelW=68, m_focusRow=-1;
  wxString *m_fields[4];

  ComposeHeader(wxWindow *p) : wxWindow(p,wxID_ANY) {
    m_fields[0]=&m_to; m_fields[1]=&m_cc; m_fields[2]=&m_bcc; m_fields[3]=&m_subject;
    SetBackgroundStyle(wxBG_STYLE_PAINT); SetMinSize(wxSize(-1,m_rowH*7+12));
    Bind(wxEVT_PAINT, &ComposeHeader::OnPaint, this);
    Bind(wxEVT_LEFT_DOWN, [this](wxMouseEvent &e) {
      int row=(e.GetY()-6)/m_rowH; if (row>=1&&row<=4) { m_focusRow=row-1; Refresh(); SetFocus(); }
    });
    Bind(wxEVT_CHAR, [this](wxKeyEvent &e) {
      if (m_focusRow<0||m_focusRow>3) { e.Skip(); return; }
      wxString *f=m_fields[m_focusRow]; int key=e.GetKeyCode();
      if (key==WXK_BACK) { if (!f->empty()) f->RemoveLast(); }
      else if (key==WXK_TAB) { m_focusRow=(m_focusRow+1)%4; }
      else if (key>=32&&key<127) { *f+=(wxChar)key; }
      else { e.Skip(); return; }
      Refresh();
    });
  }
  void SetTheme(const Theme &t) {
    m_bg=t.headerBg; m_fg=t.headerFg; m_dim=t.headerDim; m_border=t.headerBorder;
    m_inputBg=t.inputBg; m_inputFg=t.inputFg; m_labelFg=t.labelFg; m_accent=t.accentBg; Refresh();
  }
  void Reset(const wxString &to="", const wxString &cc="", const wxString &bcc="",
             const wxString &subj="", const wxString &attach="", const wxString &pri="Normal") {
    m_to=to; m_cc=cc; m_bcc=bcc; m_subject=subj; m_attach=attach; m_priority=pri; m_focusRow=0; Refresh();
  }
private:
  void OnPaint(wxPaintEvent&) {
    wxAutoBufferedPaintDC dc(this); wxRect r=GetClientRect();
    dc.SetBrush(wxBrush(m_bg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(r);
    dc.SetPen(wxPen(m_border)); dc.DrawLine(0,r.height-1,r.width,r.height-1);
    wxFont nf=GetFont(), sf=nf.Scaled(0.9f);
    wxString labels[]={"From:","To:","Cc:","Bcc:","Subject:","Attach:","Priority:"};
    wxString values[]={m_from,m_to,m_cc,m_bcc,m_subject,m_attach,m_priority};
    bool editable[]={false,true,true,true,true,false,false};
    for (int i=0;i<7;++i) {
      int y=6+i*m_rowH;
      dc.SetFont(sf); dc.SetTextForeground(m_labelFg);
      dc.DrawText(labels[i],10,y+(m_rowH-dc.GetCharHeight())/2);
      if (editable[i]) {
        bool focused=(m_focusRow==i-1);
        wxRect ir(m_labelW,y+2,r.width-m_labelW-10,m_rowH-4);
        dc.SetBrush(wxBrush(m_inputBg)); dc.SetPen(focused?wxPen(m_accent,2):wxPen(m_border));
        dc.DrawRoundedRectangle(ir,4);
        dc.SetFont(nf); dc.SetTextForeground(m_inputFg);
        dc.SetClippingRegion(ir.Deflate(6,0));
        dc.DrawText(values[i],ir.x+6,y+(m_rowH-dc.GetCharHeight())/2);
        if (focused) { int cx2=ir.x+6+dc.GetTextExtent(values[i]).x; dc.SetPen(wxPen(m_accent)); dc.DrawLine(cx2,y+4,cx2,y+m_rowH-4); }
        dc.DestroyClippingRegion();
      } else {
        dc.SetFont(nf); dc.SetTextForeground(i==5?m_dim:m_fg);
        dc.DrawText(values[i],m_labelW+6,y+(m_rowH-dc.GetCharHeight())/2);
      }
    }
  }
};

// ===== Contact card list (modern Outlook People-style) =====
class ContactCardList : public wxWindow {
public:
  wxColour m_bg, m_fg, m_dim, m_selBg, m_selFg, m_hoverBg, m_border, m_cardBg, m_accent, m_greenBg;
  wxColour m_searchBg, m_searchBorder;
  int m_sel=0, m_hov=-1, m_cardH=68, m_searchH=40, m_sectionH=26, m_scrollY=0;
  std::function<void(int)> onSelect;

  // Build sorted index with section headers
  struct Row { int contactIdx; char sectionChar; bool isSection; };
  std::vector<Row> m_rows;

  ContactCardList(wxWindow *p) : wxWindow(p,wxID_ANY) {
    SetBackgroundStyle(wxBG_STYLE_PAINT); SetCursor(wxCURSOR_HAND);
    BuildRows();
    Bind(wxEVT_PAINT, &ContactCardList::OnPaint, this);
    Bind(wxEVT_LEFT_DOWN, [this](wxMouseEvent &e) {
      int y=e.GetY()+m_scrollY-m_searchH;
      int cy=0;
      for (int i=0;i<(int)m_rows.size();++i) {
        int rh=m_rows[i].isSection?m_sectionH:m_cardH;
        if (y>=cy&&y<cy+rh&&!m_rows[i].isSection) {
          m_sel=m_rows[i].contactIdx; Refresh();
          if (onSelect) onSelect(m_sel);
          return;
        }
        cy+=rh;
      }
    });
    Bind(wxEVT_MOTION, [this](wxMouseEvent &e) {
      int y=e.GetY()+m_scrollY-m_searchH, cy=0, hov=-1;
      for (int i=0;i<(int)m_rows.size();++i) {
        int rh=m_rows[i].isSection?m_sectionH:m_cardH;
        if (y>=cy&&y<cy+rh&&!m_rows[i].isSection) { hov=m_rows[i].contactIdx; break; }
        cy+=rh;
      }
      if (hov!=m_hov) { m_hov=hov; Refresh(); }
    });
    Bind(wxEVT_LEAVE_WINDOW, [this](wxMouseEvent&) { m_hov=-1; Refresh(); });
    Bind(wxEVT_MOUSEWHEEL, [this](wxMouseEvent &e) {
      m_scrollY-=e.GetWheelRotation()/3;
      if (m_scrollY<0) m_scrollY=0;
      Refresh();
    });
  }
  void BuildRows() {
    m_rows.clear();
    // Sort contacts alphabetically by name
    std::vector<int> sorted;
    for (int i=0;i<kNumContacts;++i) sorted.push_back(i);
    std::sort(sorted.begin(),sorted.end(),[](int a,int b){ return strcmp(kContacts[a].name,kContacts[b].name)<0; });
    char lastLetter=0;
    for (int idx:sorted) {
      char c=toupper(kContacts[idx].name[0]);
      if (c!=lastLetter) { m_rows.push_back({-1,c,true}); lastLetter=c; }
      m_rows.push_back({idx,0,false});
    }
  }
  void SetTheme(const Theme &t) {
    m_bg=t.listBg; m_fg=t.listFg; m_dim=t.listDim; m_selBg=t.listSelBg; m_selFg=t.listSelFg;
    m_hoverBg=t.listHoverBg; m_border=t.listGrid; m_cardBg=t.cardBg; m_accent=t.accentBg;
    m_greenBg=t.greenBg; m_searchBg=t.inputBg; m_searchBorder=t.headerBorder; Refresh();
  }
private:
  void OnPaint(wxPaintEvent&) {
    wxAutoBufferedPaintDC dc(this); wxRect cr=GetClientRect();
    dc.SetBrush(wxBrush(m_bg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(cr);
    wxFont nf=GetFont(), bf=nf.Bold(), sf=nf.Scaled(0.82f), lf=nf.Scaled(1.1f), secf=nf.Bold().Scaled(0.85f);

    // Search bar at top
    dc.SetBrush(wxBrush(m_searchBg)); dc.SetPen(wxPen(m_searchBorder));
    dc.DrawRoundedRectangle(8,8,cr.width-16,m_searchH-16,6);
    dc.SetFont(sf); dc.SetTextForeground(m_dim);
    dc.DrawText("\xF0\x9F\x94\x8D  Search contacts...",16,14);

    int y=m_searchH-m_scrollY;
    for (int i=0;i<(int)m_rows.size();++i) {
      const auto &row=m_rows[i];
      if (row.isSection) {
        // Section header (alphabetical letter)
        if (y+m_sectionH>0&&y<cr.height) {
          dc.SetBrush(wxBrush(m_bg)); dc.SetPen(*wxTRANSPARENT_PEN);
          dc.DrawRectangle(0,y,cr.width,m_sectionH);
          dc.SetFont(secf); dc.SetTextForeground(m_accent);
          dc.DrawText(wxString(row.sectionChar),14,y+(m_sectionH-dc.GetCharHeight())/2);
          dc.SetPen(wxPen(m_border)); dc.DrawLine(28,y+m_sectionH-1,cr.width-8,y+m_sectionH-1);
        }
        y+=m_sectionH;
        continue;
      }

      if (y+m_cardH>0&&y<cr.height) {
        const auto &c=kContacts[row.contactIdx];
        bool sel=(row.contactIdx==m_sel), hov=(row.contactIdx==m_hov&&!sel);

        // Selection / hover
        if (sel) {
          dc.SetBrush(wxBrush(m_selBg)); dc.SetPen(*wxTRANSPARENT_PEN);
          dc.DrawRoundedRectangle(4,y+2,cr.width-8,m_cardH-4,6);
          dc.SetBrush(wxBrush(m_accent)); dc.DrawRectangle(4,y+8,3,m_cardH-16);
        } else if (hov) {
          dc.SetBrush(wxBrush(m_hoverBg)); dc.SetPen(*wxTRANSPARENT_PEN);
          dc.DrawRoundedRectangle(4,y+2,cr.width-8,m_cardH-4,6);
        }

        // Avatar circle with online status dot
        int avX=32, avY=y+m_cardH/2, avR=22;
        dc.SetBrush(wxBrush(c.avatarColor)); dc.SetPen(*wxTRANSPARENT_PEN);
        dc.DrawCircle(avX,avY,avR);
        dc.SetFont(lf); dc.SetTextForeground(*wxWHITE);
        wxString name2=c.name; wxString init; init+=name2[0];
        int sp=name2.Find(' '); if (sp!=wxNOT_FOUND&&sp+1<(int)name2.length()) init+=name2[sp+1];
        wxSize is=dc.GetTextExtent(init);
        dc.DrawText(init,avX-is.x/2,avY-is.y/2);
        // Online status dot (green for some contacts)
        if (row.contactIdx%3==0) {
          dc.SetBrush(wxBrush(m_greenBg)); dc.SetPen(wxPen(m_bg,2));
          dc.DrawCircle(avX+avR-3,avY+avR-3,5);
        }

        // Name + quick action icons on right
        int tx=62;
        dc.SetFont(bf); dc.SetTextForeground(sel?m_selFg:m_fg);
        dc.DrawText(c.name,tx,y+12);

        // Right side: email + chat quick action icons
        if (hov||sel) {
          dc.SetFont(sf); dc.SetTextForeground(sel?m_selFg:m_accent);
          int rx=cr.width-56;
          dc.DrawText("\xE2\x9C\x89",rx,y+14); // mail icon
          dc.DrawText("\xF0\x9F\x92\xAC",rx+20,y+14); // chat icon
        }

        // Title and company
        dc.SetFont(sf); dc.SetTextForeground(sel?m_selFg:m_fg);
        wxString line2;
        if (strlen(c.title)>0) line2=c.title;
        if (strlen(c.company)>0) { if (!line2.empty()) line2+=" \xE2\x80\xA2 "; line2+=c.company; }
        if (!line2.empty()) {
          dc.SetClippingRegion(tx,y+30,cr.width-tx-60,16);
          dc.DrawText(line2,tx,y+30);
          dc.DestroyClippingRegion();
        }

        // Email
        dc.SetTextForeground(sel?m_selFg:m_dim);
        dc.SetClippingRegion(tx,y+46,cr.width-tx-10,16);
        dc.DrawText(c.email,tx,y+46);
        dc.DestroyClippingRegion();
      }
      y+=m_cardH;
    }
  }
};

// ===== Contact detail panel (right side, rich card) =====
class ContactDetail : public wxWindow {
public:
  wxColour m_bg, m_fg, m_dim, m_border, m_accent, m_cardBg, m_greenBg, m_hoverBg;
  int m_idx=0, m_btnY=230;
  std::function<void(const wxString&)> onComposeTo;

  ContactDetail(wxWindow *p) : wxWindow(p,wxID_ANY) {
    SetBackgroundStyle(wxBG_STYLE_PAINT);
    Bind(wxEVT_PAINT, &ContactDetail::OnPaint, this);
    Bind(wxEVT_LEFT_DOWN, [this](wxMouseEvent &e) {
      if (m_idx>=0&&m_idx<kNumContacts) {
        if (e.GetY()>=m_btnY&&e.GetY()<=m_btnY+34) {
          wxRect cr=GetClientRect();
          int cardX=20, cardW=std::min(cr.width-40,440);
          int bw=(cardW-48)/3;
          if (e.GetX()>=cardX+12&&e.GetX()<=cardX+12+bw) {
            if (onComposeTo) onComposeTo(kContacts[m_idx].email);
          }
        }
      }
    });
    SetCursor(wxCURSOR_HAND);
  }
  void SetContact(int idx) { m_idx=idx; Refresh(); }
  void SetTheme(const Theme &t) {
    m_bg=t.panelBg; m_fg=t.panelFg; m_dim=t.panelDim; m_border=t.headerBorder;
    m_accent=t.accentBg; m_cardBg=t.cardBg; m_greenBg=t.greenBg; m_hoverBg=t.listHoverBg; Refresh();
  }
private:
  void OnPaint(wxPaintEvent&) {
    wxAutoBufferedPaintDC dc(this); wxRect r=GetClientRect();
    dc.SetBrush(wxBrush(m_bg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(r);
    if (m_idx<0||m_idx>=kNumContacts) return;
    const auto &c=kContacts[m_idx];
    wxFont nf=GetFont(), bf=nf.Bold(), sf=nf.Scaled(0.85f), lf=nf.Scaled(1.5f), xlf=nf.Scaled(2.2f), hf=bf.Scaled(0.78f);

    int cardX=20, cardY=16, cardW=std::min(r.width-40,440);

    // Header banner with gradient-like colored stripe
    int bannerH=80;
    dc.SetBrush(wxBrush(c.avatarColor)); dc.SetPen(*wxTRANSPARENT_PEN);
    dc.DrawRoundedRectangle(cardX,cardY,cardW,bannerH,8);
    // Square off bottom corners
    dc.DrawRectangle(cardX,cardY+bannerH-8,cardW,8);

    // Card body
    int bodyY=cardY+bannerH;
    dc.SetBrush(wxBrush(m_cardBg)); dc.SetPen(wxPen(m_border));
    dc.DrawRectangle(cardX,bodyY,cardW,1); // top line under banner
    dc.SetPen(*wxTRANSPARENT_PEN);

    // Large avatar overlapping banner
    int avX=cardX+cardW/2, avY=bodyY-4, avR=36;
    dc.SetBrush(wxBrush(m_cardBg)); dc.SetPen(*wxTRANSPARENT_PEN);
    dc.DrawCircle(avX,avY,avR+3); // white ring
    dc.SetBrush(wxBrush(c.avatarColor));
    dc.DrawCircle(avX,avY,avR);
    dc.SetFont(xlf); dc.SetTextForeground(*wxWHITE);
    wxString name2=c.name; wxString init; init+=name2[0];
    int sp=name2.Find(' '); if (sp!=wxNOT_FOUND&&sp+1<(int)name2.length()) init+=name2[sp+1];
    wxSize is=dc.GetTextExtent(init);
    dc.DrawText(init,avX-is.x/2,avY-is.y/2);
    // Online dot
    if (m_idx%3==0) {
      dc.SetBrush(wxBrush(m_greenBg)); dc.SetPen(wxPen(m_cardBg,2));
      dc.DrawCircle(avX+avR-4,avY+avR-4,6);
    }

    // Card content area
    int contentY=avY+avR+8;
    int cardBottom=contentY+240;
    dc.SetBrush(wxBrush(m_cardBg)); dc.SetPen(wxPen(m_border));
    dc.DrawRoundedRectangle(cardX,bodyY,cardW,cardBottom-bodyY,0);
    // round bottom
    dc.SetBrush(wxBrush(m_cardBg)); dc.SetPen(*wxTRANSPARENT_PEN);
    dc.DrawRoundedRectangle(cardX,cardBottom-16,cardW,16,8);
    dc.DrawRectangle(cardX,cardBottom-16,cardW,8);

    // Name centered
    dc.SetFont(bf.Scaled(1.2f)); dc.SetTextForeground(m_fg);
    wxSize ns=dc.GetTextExtent(c.name);
    dc.DrawText(c.name,avX-ns.x/2,contentY);
    contentY+=ns.y+2;

    // Title + Company centered
    if (strlen(c.title)>0||strlen(c.company)>0) {
      dc.SetFont(sf); dc.SetTextForeground(m_dim);
      wxString tc;
      if (strlen(c.title)>0) tc=c.title;
      if (strlen(c.company)>0) { if (!tc.empty()) tc+=" at "; tc+=c.company; }
      wxSize tcs=dc.GetTextExtent(tc);
      dc.DrawText(tc,avX-tcs.x/2,contentY);
      contentY+=tcs.y+2;
    }

    // Online status text
    if (m_idx%3==0) {
      dc.SetFont(hf); dc.SetTextForeground(m_greenBg);
      wxString onl="\xE2\x97\x8F Online";
      wxSize os=dc.GetTextExtent(onl);
      dc.DrawText(onl,avX-os.x/2,contentY);
    }
    contentY+=20;

    // Divider
    dc.SetPen(wxPen(m_border)); dc.DrawLine(cardX+16,contentY,cardX+cardW-16,contentY);
    contentY+=12;

    // Info section with icons
    dc.SetFont(nf);
    auto drawInfoRow=[&](const wxString &icon, const wxString &label, const wxString &val, const wxColour &valCol) {
      if (val.empty()) return;
      dc.SetTextForeground(m_accent); dc.DrawText(icon,cardX+20,contentY);
      dc.SetFont(hf); dc.SetTextForeground(m_dim);
      dc.DrawText(label,cardX+42,contentY+1);
      dc.SetFont(nf); dc.SetTextForeground(valCol);
      dc.DrawText(val,cardX+42,contentY+16);
      contentY+=36;
    };
    drawInfoRow("\xE2\x9C\x89","EMAIL",c.email,m_accent);
    drawInfoRow("\xF0\x9F\x93\x9E","PHONE",c.phone,m_fg);
    if (strlen(c.company)>0)
      drawInfoRow("\xF0\x9F\x8F\xA2","COMPANY",c.company,m_fg);

    // Notes section
    if (strlen(c.notes)>0) {
      dc.SetPen(wxPen(m_border)); dc.DrawLine(cardX+16,contentY,cardX+cardW-16,contentY);
      contentY+=10;
      dc.SetFont(hf); dc.SetTextForeground(m_dim);
      dc.DrawText("NOTES",cardX+20,contentY); contentY+=16;
      dc.SetFont(sf); dc.SetTextForeground(m_fg);
      dc.DrawText(c.notes,cardX+20,contentY);
      contentY+=20;
    }

    // Action buttons row
    contentY+=12;
    m_btnY=contentY;
    int bw=(cardW-48)/3;
    // "Send Email" - primary
    dc.SetBrush(wxBrush(m_accent)); dc.SetPen(*wxTRANSPARENT_PEN);
    dc.DrawRoundedRectangle(cardX+12,contentY,bw,34,6);
    dc.SetFont(sf); dc.SetTextForeground(*wxWHITE);
    wxString seLbl="\xE2\x9C\x89 Email";
    wxSize seS=dc.GetTextExtent(seLbl);
    dc.DrawText(seLbl,cardX+12+(bw-seS.x)/2,contentY+8);
    // "Call" - outline
    dc.SetBrush(wxBrush(m_cardBg)); dc.SetPen(wxPen(m_border));
    dc.DrawRoundedRectangle(cardX+12+bw+6,contentY,bw,34,6);
    dc.SetTextForeground(m_fg);
    wxString clLbl="\xF0\x9F\x93\x9E Call";
    wxSize clS=dc.GetTextExtent(clLbl);
    dc.DrawText(clLbl,cardX+12+bw+6+(bw-clS.x)/2,contentY+8);
    // "Edit" - outline
    dc.SetBrush(wxBrush(m_cardBg)); dc.SetPen(wxPen(m_border));
    dc.DrawRoundedRectangle(cardX+12+2*(bw+6),contentY,bw,34,6);
    dc.SetTextForeground(m_dim);
    wxString edLbl="\xE2\x9C\x8F Edit";
    wxSize edS=dc.GetTextExtent(edLbl);
    dc.DrawText(edLbl,cardX+12+2*(bw+6)+(bw-edS.x)/2,contentY+8);

    // Recent emails section below the card
    contentY+=54;
    dc.SetFont(hf); dc.SetTextForeground(m_dim);
    dc.DrawText("RECENT EMAILS",cardX+4,contentY); contentY+=18;
    dc.SetPen(wxPen(m_border));
    // Show 2 dummy recent emails
    wxString recentSubj[]={"Re: Welcome to Eudora 2026!","Dashboard redesign - timeline update"};
    wxString recentDate[]={"Mar 7, 2026","Mar 6, 2026"};
    for (int i=0;i<2;++i) {
      dc.SetBrush(wxBrush(m_cardBg)); dc.SetPen(wxPen(m_border));
      dc.DrawRoundedRectangle(cardX,contentY,cardW,36,4);
      dc.SetFont(sf); dc.SetTextForeground(m_fg);
      dc.DrawText(recentSubj[i],cardX+10,contentY+4);
      dc.SetFont(hf); dc.SetTextForeground(m_dim);
      dc.DrawText(recentDate[i],cardX+10,contentY+20);
      contentY+=42;
    }
  }
};

// ===== Filter list (rule editor style) =====
class FilterList : public wxWindow {
public:
  wxColour m_bg, m_fg, m_dim, m_selBg, m_selFg, m_hoverBg, m_grid, m_stripe, m_accent, m_greenBg, m_redBg, m_orangeBg, m_cardBg, m_border;
  int m_sel=0, m_hov=-1, m_cardH=96, m_headerH=44;

  FilterList(wxWindow *p) : wxWindow(p,wxID_ANY) {
    SetBackgroundStyle(wxBG_STYLE_PAINT); SetCursor(wxCURSOR_HAND);
    Bind(wxEVT_PAINT, &FilterList::OnPaint, this);
    Bind(wxEVT_LEFT_DOWN, [this](wxMouseEvent &e) {
      int y=e.GetY()-m_headerH;
      if (y<0) return;
      int idx=y/m_cardH;
      if (idx>=0&&idx<kNumFilters) {
        // Toggle switch area
        int cardX=12, swX=cardX+cardX+8;
        if (e.GetX()>=swX&&e.GetX()<=swX+40) {
          kFilters[idx].enabled=!kFilters[idx].enabled;
        }
        m_sel=idx; Refresh();
      }
    });
    Bind(wxEVT_MOTION, [this](wxMouseEvent &e) {
      int y=e.GetY()-m_headerH; int idx=y>=0?y/m_cardH:-1;
      if (idx>=kNumFilters) idx=-1;
      if (idx!=m_hov) { m_hov=idx; Refresh(); }
    });
    Bind(wxEVT_LEAVE_WINDOW, [this](wxMouseEvent&) { m_hov=-1; Refresh(); });
  }
  void SetTheme(const Theme &t) {
    m_bg=t.listBg; m_fg=t.listFg; m_dim=t.listDim; m_selBg=t.listSelBg; m_selFg=t.listSelFg;
    m_hoverBg=t.listHoverBg; m_grid=t.listGrid; m_stripe=t.listStripe; m_accent=t.accentBg;
    m_greenBg=t.greenBg; m_redBg=t.redBg; m_orangeBg=t.orangeBg; m_cardBg=t.cardBg; m_border=t.cardBorder; Refresh();
  }
private:
  void OnPaint(wxPaintEvent&) {
    wxAutoBufferedPaintDC dc(this); wxRect cr=GetClientRect();
    dc.SetBrush(wxBrush(m_bg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(cr);
    wxFont nf=GetFont(), bf=nf.Bold(), sf=nf.Scaled(0.82f), tf=nf.Scaled(0.75f), hf=bf.Scaled(1.05f), labf=bf.Scaled(0.72f);

    // Header area
    dc.SetFont(hf); dc.SetTextForeground(m_fg);
    dc.DrawText("\xF0\x9F\x93\x8B Mail Filters",16,12);
    // "Add Rule" button in header
    dc.SetFont(sf);
    wxString addLbl="+ New Rule";
    wxSize als=dc.GetTextExtent(addLbl);
    int abx=cr.width-als.x-24, aby=10;
    dc.SetBrush(wxBrush(m_accent)); dc.SetPen(*wxTRANSPARENT_PEN);
    dc.DrawRoundedRectangle(abx,aby,als.x+16,22,4);
    dc.SetTextForeground(*wxWHITE);
    dc.DrawText(addLbl,abx+8,aby+3);
    // Summary
    int active=(int)std::count_if(kFilters,kFilters+kNumFilters,[](const Filter &f){return f.enabled;});
    dc.SetFont(tf); dc.SetTextForeground(m_dim);
    dc.DrawText(wxString::Format("%d rules \xE2\x80\xA2 %d active \xE2\x80\xA2 %d disabled",kNumFilters,active,kNumFilters-active),16,32);

    for (int i=0;i<kNumFilters;++i) {
      int y=m_headerH+i*m_cardH; if (y>cr.height) break;
      const auto &f=kFilters[i]; bool sel=(i==m_sel), hov=(i==m_hov&&!sel);

      // Card
      wxRect card(12,y+4,cr.width-24,m_cardH-8);
      if (sel) {
        dc.SetBrush(wxBrush(m_cardBg)); dc.SetPen(wxPen(m_accent,2));
        dc.DrawRoundedRectangle(card,6);
      } else {
        dc.SetBrush(wxBrush(hov?m_hoverBg:m_cardBg)); dc.SetPen(wxPen(m_border));
        dc.DrawRoundedRectangle(card,6);
      }

      // Left color bar indicating status
      wxColour barCol=f.enabled?m_greenBg:m_dim;
      dc.SetBrush(wxBrush(barCol)); dc.SetPen(*wxTRANSPARENT_PEN);
      dc.DrawRoundedRectangle(card.x,card.y,4,card.height,2);

      // Toggle switch
      int swX=card.x+16, swY=y+18, swW=38, swH=20;
      wxColour trackCol=f.enabled?m_greenBg:m_dim;
      dc.SetBrush(wxBrush(trackCol)); dc.SetPen(*wxTRANSPARENT_PEN);
      dc.DrawRoundedRectangle(swX,swY,swW,swH,swH/2);
      int knobX=f.enabled?swX+swW-swH+2:swX+2;
      dc.SetBrush(wxBrush(*wxWHITE));
      dc.DrawCircle(knobX+swH/2-2,swY+swH/2,swH/2-3);

      // Rule number badge
      int tx=swX+swW+14;
      dc.SetFont(tf); dc.SetTextForeground(m_dim);
      wxString ruleNum=wxString::Format("#%d",i+1);
      dc.DrawText(ruleNum,tx,y+12);

      // Name
      dc.SetFont(bf); dc.SetTextForeground(f.enabled?m_fg:m_dim);
      dc.DrawText(f.name,tx+30,y+10);

      // Condition with label pill
      int condY=y+32;
      dc.SetFont(labf); dc.SetTextForeground(*wxWHITE);
      dc.SetBrush(wxBrush(m_accent)); dc.SetPen(*wxTRANSPARENT_PEN);
      dc.DrawRoundedRectangle(tx,condY,18,14,3);
      dc.DrawText("IF",tx+3,condY+1);
      dc.SetFont(sf); dc.SetTextForeground(f.enabled?m_fg:m_dim);
      dc.DrawText(f.match,tx+24,condY);

      // Arrow
      int arrY=condY+17;
      dc.SetTextForeground(m_accent);
      dc.SetFont(sf); dc.DrawText("\xE2\x86\x93",tx+4,arrY);

      // Action with label pill
      int actY=arrY+16;
      dc.SetFont(labf); dc.SetTextForeground(*wxWHITE);
      wxColour actPill=m_greenBg;
      wxString actStr=f.action;
      if (actStr.Contains("Junk")||actStr.Contains("Skip")) actPill=m_orangeBg;
      dc.SetBrush(wxBrush(actPill)); dc.SetPen(*wxTRANSPARENT_PEN);
      dc.DrawRoundedRectangle(tx,actY,30,14,3);
      dc.DrawText("THEN",tx+2,actY+1);
      dc.SetFont(sf); dc.SetTextForeground(f.enabled?m_accent:m_dim);
      dc.DrawText(f.action,tx+36,actY);

      // Right side action icons (on hover/select)
      if (hov||sel) {
        dc.SetFont(sf); dc.SetTextForeground(m_dim);
        int rx=card.x+card.width-60;
        dc.DrawText("\xE2\x9C\x8F",rx,y+16); // edit
        dc.DrawText("\xF0\x9F\x93\x8B",rx+20,y+16); // duplicate
        dc.DrawText("\xF0\x9F\x97\x91",rx+40,y+16); // delete
      }

      // Status pill
      wxString statusText=f.enabled?"Active":"Off";
      wxColour pillBg=f.enabled?m_greenBg:m_dim;
      dc.SetFont(tf);
      wxSize ps=dc.GetTextExtent(statusText);
      int px=card.x+card.width-ps.x-16, py=y+m_cardH-22;
      dc.SetBrush(wxBrush(pillBg)); dc.SetPen(*wxTRANSPARENT_PEN);
      dc.DrawRoundedRectangle(px-4,py,ps.x+8,ps.y+4,8);
      dc.SetTextForeground(*wxWHITE);
      dc.DrawText(statusText,px,py+2);
    }
  }
};

// ===== Status bar =====
class StatusBar : public wxWindow {
public:
  wxColour m_bg, m_fg, m_border, m_dim;
  wxString m_left, m_center, m_right;
  StatusBar(wxWindow *p) : wxWindow(p,wxID_ANY) {
    SetBackgroundStyle(wxBG_STYLE_PAINT); SetMinSize(wxSize(-1,24));
    Bind(wxEVT_PAINT, &StatusBar::OnPaint, this);
  }
  void SetTheme(const Theme &t) { m_bg=t.statusBg; m_fg=t.statusFg; m_border=t.statusBorder; m_dim=t.panelDim; Refresh(); }
  void Set(const wxString &l, const wxString &c="", const wxString &r="") { m_left=l; m_center=c; m_right=r; Refresh(); }
private:
  void OnPaint(wxPaintEvent&) {
    wxAutoBufferedPaintDC dc(this); wxRect r=GetClientRect();
    dc.SetBrush(wxBrush(m_bg)); dc.SetPen(*wxTRANSPARENT_PEN); dc.DrawRectangle(r);
    dc.SetPen(wxPen(m_border)); dc.DrawLine(0,0,r.width,0);
    wxFont f=GetFont().Scaled(0.82f); dc.SetFont(f);
    int y=(r.height-dc.GetCharHeight())/2+1;
    dc.SetTextForeground(m_fg); dc.DrawText(m_left,10,y);
    if (!m_center.empty()) { dc.SetTextForeground(m_dim); wxSize cs=dc.GetTextExtent(m_center); dc.DrawText(m_center,(r.width-cs.x)/2,y); }
    if (!m_right.empty()) { dc.SetTextForeground(m_dim); wxSize rs=dc.GetTextExtent(m_right); dc.DrawText(m_right,r.width-rs.x-10,y); }
    dc.SetPen(wxPen(m_border)); dc.DrawLine(r.width/3,4,r.width/3,r.height-4); dc.DrawLine(2*r.width/3,4,2*r.width/3,r.height-4);
  }
};

// ===== HTML builders =====
static wxString ViewHTML(const Email &e) {
  return wxString::Format("<body style=\"font-family:-apple-system,Segoe UI,Helvetica,Arial,sans-serif;"
    "font-size:10.5pt;padding:16px 20px;line-height:1.6;\">%s</body>",e.body);
}
static wxString ReplyHTML(const Email &e) {
  wxString h="<body style=\"font-family:-apple-system,Segoe UI,Helvetica,Arial,sans-serif;"
    "font-size:10.5pt;padding:14px 18px;line-height:1.6;\"><p><br/></p>"
    "<div style=\"margin-top:20px;padding-top:8px;border-top:1px solid currentColor;opacity:0.3;\">"
    "<div style=\"font-size:9pt;\">-- <br/>Sent with Eudora 2026</div></div>";
  h+=wxString::Format("<div style=\"font-size:9pt;opacity:0.5;margin:16px 0 4px 0;\">"
    "At %s, %s wrote:</div>",e.date,e.from);
  // Split body into paragraphs for inline reply - each <p> becomes its own blockquote
  // with an empty unquoted <p> between them for cursor insertion
  wxString body=e.body;
  wxString bqOpen="<blockquote style=\"margin:0;padding:0 0 0 12px;border-left:3px solid #326ea5;\">";
  wxString bqClose="</blockquote>";
  // If body already contains blockquotes (nested quoting), wrap the whole thing
  if (body.Contains("<blockquote")) {
    h+=bqOpen+body+bqClose;
  } else {
    // Split on </p> to create individual quoted paragraphs with insertion points
    wxString remaining=body;
    bool first=true;
    while (!remaining.empty()) {
      int pos=remaining.Find("</p>");
      if (pos==wxNOT_FOUND) {
        if (!first) h+="<p><br/></p>";
        h+=bqOpen+remaining+bqClose;
        break;
      }
      wxString chunk=remaining.Left(pos+4);
      remaining=remaining.Mid(pos+4);
      if (!chunk.Trim().empty()) {
        if (!first) h+="<p><br/></p>";
        h+=bqOpen+chunk+bqClose;
        first=false;
      }
    }
  }
  h+="<p><br/></p></body>"; return h;
}
static wxString NewHTML() {
  return "<body style=\"font-family:-apple-system,Segoe UI,Helvetica,Arial,sans-serif;"
    "font-size:10.5pt;padding:14px 18px;line-height:1.6;\"><p><br/></p><p><br/></p>"
    "<div style=\"margin-top:20px;padding-top:8px;border-top:1px solid currentColor;opacity:0.3;\">"
    "<div style=\"font-size:9pt;\">-- <br/>Sent with Eudora 2026</div></div></body>";
}

// ===== App =====
class EudoraApp : public wxApp {
public:
  bool OnInit() override {
    wxInitAllImageHandlers();
    auto *frame=new wxFrame(nullptr,wxID_ANY,"Eudora 2026",wxDefaultPosition,wxSize(1360,880));
    auto *root=new wxPanel(frame); root->SetBackgroundColour(kLight.toolbarBg);
    auto *rootSz=new wxBoxSizer(wxVERTICAL);

    // Toolbar
    auto *tb=new wxPanel(root); tb->SetBackgroundColour(kLight.toolbarBg);
    auto *tbSz=new wxBoxSizer(wxHORIZONTAL);
    auto *bCheck=new TBtn(tb,wxID_ANY,"Check Mail","\xF0\x9F\x93\xA5"); bCheck->m_sep=true;
    auto *bNew=new TBtn(tb,wxID_ANY,"New Message","\xE2\x9C\x8F");
    bNew->m_accent=true; bNew->m_bg=wxColour(50,110,165); bNew->m_hoverBg=wxColour(40,95,148); bNew->m_pressedBg=wxColour(30,80,130);
    auto *bReply=new TBtn(tb,wxID_ANY,"Reply","\xE2\x86\xA9");
    auto *bReplyAll=new TBtn(tb,wxID_ANY,"Reply All","\xE2\x87\x8F");
    auto *bFwd=new TBtn(tb,wxID_ANY,"Forward","\xE2\x86\x92"); bFwd->m_sep=true;
    auto *bDel=new TBtn(tb,wxID_ANY,"Trash","\xF0\x9F\x97\x91");
    auto *bJunk=new TBtn(tb,wxID_ANY,"Junk","\xE2\x9B\x94"); bJunk->m_sep=true;
    auto *bPrint=new TBtn(tb,wxID_ANY,"Print","\xF0\x9F\x96\xA8");
    auto *bFind=new TBtn(tb,wxID_ANY,"Find","\xF0\x9F\x94\x8D"); bFind->m_sep=true;
    auto *bDark=new TBtn(tb,wxID_ANY,"","\xE2\x98\xBD",wxSize(36,36));

    auto setTBTheme=[=](const Theme &t) {
      tb->SetBackgroundColour(t.toolbarBg);
      for (auto *b:{bCheck,bReply,bReplyAll,bFwd,bDel,bJunk,bPrint,bFind,bDark})
        b->SetColors(t.toolbarBg,t.toolbarFg,t.toolbarHover);
      tb->Refresh();
    };
    setTBTheme(kLight);

    tbSz->AddSpacer(4);
    tbSz->Add(bCheck,0,wxALL,1); tbSz->AddSpacer(2);
    tbSz->Add(bNew,0,wxALL,1); tbSz->AddSpacer(4);
    tbSz->Add(bReply,0,wxALL,1); tbSz->Add(bReplyAll,0,wxALL,1); tbSz->Add(bFwd,0,wxALL,1);
    tbSz->AddSpacer(2); tbSz->Add(bDel,0,wxALL,1); tbSz->Add(bJunk,0,wxALL,1);
    tbSz->AddSpacer(2); tbSz->Add(bPrint,0,wxALL,1); tbSz->Add(bFind,0,wxALL,1);
    tbSz->AddStretchSpacer(); tbSz->Add(bDark,0,wxALL|wxALIGN_CENTER_VERTICAL,1); tbSz->AddSpacer(4);
    tb->SetSizer(tbSz);

    auto *tbLine=new wxPanel(root,wxID_ANY,wxDefaultPosition,wxSize(-1,1));
    tbLine->SetBackgroundColour(kLight.toolbarBorder);
    rootSz->Add(tb,0,wxEXPAND); rootSz->Add(tbLine,0,wxEXPAND);

    // Content: Wazoo | sep | right
    auto *content=new wxPanel(root);
    auto *cSz=new wxBoxSizer(wxHORIZONTAL);
    auto *wazoo=new Wazoo(content); wazoo->SetTheme(kLight);
    auto *wSep=new wxPanel(content,wxID_ANY,wxDefaultPosition,wxSize(1,-1));
    wSep->SetBackgroundColour(kLight.toolbarBorder);

    auto *rightP=new wxPanel(content);
    auto *rSz=new wxBoxSizer(wxVERTICAL);
    auto *tabBar=new TabBar(rightP); tabBar->SetTheme(kLight);
    tabBar->AddTab("\xF0\x9F\x93\xA5 In");
    tabBar->AddTab("\xF0\x9F\x93\xA4 Out");
    tabBar->AddTab("\xF0\x9F\x91\xA5 Contacts");
    tabBar->AddTab("\xF0\x9F\x93\x8B Filters");

    // Page 0: In
    auto *pageIn=new wxPanel(rightP); auto *piSz=new wxBoxSizer(wxVERTICAL);
    auto *msgListIn=new MsgList(pageIn); msgListIn->SetData(kInbox,kNumInbox,false); msgListIn->SetTheme(kLight);
    auto *msgHdrIn=new MsgHeader(pageIn); msgHdrIn->SetTheme(kLight); msgHdrIn->SetEmail(&kInbox[0]);
    auto *previewIn=new wxHtmlEditWidget(pageIn); previewIn->SetReadOnly(true);
    previewIn->GetEngine().defaultFont=wxFont(10,wxFONTFAMILY_DEFAULT,wxFONTSTYLE_NORMAL,wxFONTWEIGHT_NORMAL);
    previewIn->SetHTML(ViewHTML(kInbox[0]));
    piSz->Add(msgListIn,2,wxEXPAND); piSz->Add(msgHdrIn,0,wxEXPAND); piSz->Add(previewIn,3,wxEXPAND);
    pageIn->SetSizer(piSz);

    // Page 1: Out
    auto *pageOut=new wxPanel(rightP); auto *poSz=new wxBoxSizer(wxVERTICAL);
    auto *msgListOut=new MsgList(pageOut); msgListOut->SetData(kOutbox,kNumOutbox,true); msgListOut->SetTheme(kLight);
    auto *msgHdrOut=new MsgHeader(pageOut); msgHdrOut->SetTheme(kLight); msgHdrOut->SetEmail(&kOutbox[0]);
    auto *previewOut=new wxHtmlEditWidget(pageOut); previewOut->SetReadOnly(true);
    previewOut->GetEngine().defaultFont=wxFont(10,wxFONTFAMILY_DEFAULT,wxFONTSTYLE_NORMAL,wxFONTWEIGHT_NORMAL);
    previewOut->SetHTML(ViewHTML(kOutbox[0]));
    poSz->Add(msgListOut,2,wxEXPAND); poSz->Add(msgHdrOut,0,wxEXPAND); poSz->Add(previewOut,3,wxEXPAND);
    pageOut->SetSizer(poSz); pageOut->Hide();

    // Page 2: Contacts (list on left, detail card on right)
    auto *pageAddr=new wxPanel(rightP); auto *paSz=new wxBoxSizer(wxHORIZONTAL);
    auto *contactList=new ContactCardList(pageAddr); contactList->SetTheme(kLight);
    auto *addrSep=new wxPanel(pageAddr,wxID_ANY,wxDefaultPosition,wxSize(1,-1));
    addrSep->SetBackgroundColour(kLight.toolbarBorder);
    auto *contactDetail=new ContactDetail(pageAddr); contactDetail->SetTheme(kLight);
    paSz->Add(contactList,1,wxEXPAND); paSz->Add(addrSep,0,wxEXPAND); paSz->Add(contactDetail,1,wxEXPAND);
    pageAddr->SetSizer(paSz); pageAddr->Hide();

    // Page 3: Filters
    auto *pageFilt=new wxPanel(rightP); auto *pfSz=new wxBoxSizer(wxVERTICAL);
    auto *filterList=new FilterList(pageFilt); filterList->SetTheme(kLight);
    pfSz->Add(filterList,1,wxEXPAND);
    pageFilt->SetSizer(pfSz); pageFilt->Hide();

    // Page 4: Compose (dynamic tab)
    auto *pageCompose=new wxPanel(rightP); auto *pcSz=new wxBoxSizer(wxVERTICAL);
    auto *compHdr=new ComposeHeader(pageCompose); compHdr->SetTheme(kLight);
    auto *cTb=new wxPanel(pageCompose); cTb->SetBackgroundColour(kLight.toolbarBg);
    auto *ctSz=new wxBoxSizer(wxHORIZONTAL);
    auto *bSend=new TBtn(cTb,wxID_ANY,"Send","\xE2\x9C\x89",wxSize(-1,30));
    bSend->m_accent=true; bSend->m_bg=wxColour(50,110,165); bSend->m_hoverBg=wxColour(40,95,148); bSend->m_pressedBg=wxColour(30,80,130);
    auto *bQueue=new TBtn(cTb,wxID_ANY,"Queue","",wxSize(-1,30));
    auto *bDiscard=new TBtn(cTb,wxID_ANY,"Discard","",wxSize(-1,30)); bDiscard->m_sep=true;
    auto *bBold=new TBtn(cTb,wxID_ANY,"B","",wxSize(28,30)); bBold->m_font=bBold->GetFont().Bold();
    auto *bItalic=new TBtn(cTb,wxID_ANY,"I","",wxSize(28,30)); bItalic->m_font=bItalic->GetFont().MakeItalic();
    auto *bUline=new TBtn(cTb,wxID_ANY,"U","",wxSize(28,30));
    auto *bStrike=new TBtn(cTb,wxID_ANY,"S","",wxSize(28,30)); bStrike->m_sep=true;
    auto *bFontUp=new TBtn(cTb,wxID_ANY,"A+","",wxSize(32,30));
    auto *bFontDn=new TBtn(cTb,wxID_ANY,"A-","",wxSize(32,30)); bFontDn->m_sep=true;
    auto *bColor=new TBtn(cTb,wxID_ANY,"\xF0\x9F\x8E\xA8","",wxSize(30,30)); bColor->m_sep=true;
    auto *bBullet=new TBtn(cTb,wxID_ANY,"\xE2\x80\xA2","",wxSize(28,30));
    auto *bNumber=new TBtn(cTb,wxID_ANY,"1.","",wxSize(28,30)); bNumber->m_sep=true;
    auto *bQu=new TBtn(cTb,wxID_ANY,"\xC2\xBB","",wxSize(28,30));
    auto *bUnqu=new TBtn(cTb,wxID_ANY,"\xC2\xAB","",wxSize(28,30));
    auto *bHR=new TBtn(cTb,wxID_ANY,"\xE2\x80\x94","",wxSize(28,30)); bHR->m_sep=true;
    auto *bLink=new TBtn(cTb,wxID_ANY,"\xF0\x9F\x94\x97","",wxSize(28,30)); bLink->m_sep=true;
    auto *bAttBtn=new TBtn(cTb,wxID_ANY,"Attach","\xF0\x9F\x93\x8E",wxSize(-1,30));
    auto *bSig=new TBtn(cTb,wxID_ANY,"Signature","",wxSize(-1,30));

    auto setCTTheme=[=](const Theme &t) {
      cTb->SetBackgroundColour(t.toolbarBg);
      for (auto *b:{bQueue,bDiscard,bBold,bItalic,bUline,bStrike,bFontUp,bFontDn,bColor,bBullet,bNumber,bQu,bUnqu,bHR,bLink,bAttBtn,bSig})
        b->SetColors(t.toolbarBg,t.toolbarFg,t.toolbarHover);
      cTb->Refresh();
    };
    setCTTheme(kLight);
    ctSz->AddSpacer(4);
    ctSz->Add(bSend,0,wxALL,1); ctSz->Add(bQueue,0,wxALL,1); ctSz->Add(bDiscard,0,wxALL,1);
    ctSz->AddSpacer(6); ctSz->Add(bBold,0,wxALL,1); ctSz->Add(bItalic,0,wxALL,1); ctSz->Add(bUline,0,wxALL,1); ctSz->Add(bStrike,0,wxALL,1);
    ctSz->AddSpacer(4); ctSz->Add(bFontUp,0,wxALL,1); ctSz->Add(bFontDn,0,wxALL,1);
    ctSz->AddSpacer(4); ctSz->Add(bColor,0,wxALL,1);
    ctSz->AddSpacer(4); ctSz->Add(bBullet,0,wxALL,1); ctSz->Add(bNumber,0,wxALL,1);
    ctSz->AddSpacer(4); ctSz->Add(bQu,0,wxALL,1); ctSz->Add(bUnqu,0,wxALL,1); ctSz->Add(bHR,0,wxALL,1);
    ctSz->AddSpacer(4); ctSz->Add(bLink,0,wxALL,1);
    ctSz->AddSpacer(6); ctSz->Add(bAttBtn,0,wxALL,1); ctSz->Add(bSig,0,wxALL,1);
    cTb->SetSizer(ctSz);
    auto *ctLine=new wxPanel(pageCompose,wxID_ANY,wxDefaultPosition,wxSize(-1,1));
    ctLine->SetBackgroundColour(kLight.toolbarBorder);
    auto *composeW=new wxHtmlEditWidget(pageCompose); composeW->SetReadOnly(false);
    composeW->GetEngine().defaultFont=wxFont(10,wxFONTFAMILY_DEFAULT,wxFONTSTYLE_NORMAL,wxFONTWEIGHT_NORMAL);
    pcSz->Add(compHdr,0,wxEXPAND); pcSz->Add(cTb,0,wxEXPAND); pcSz->Add(ctLine,0,wxEXPAND); pcSz->Add(composeW,1,wxEXPAND);
    pageCompose->SetSizer(pcSz); pageCompose->Hide();

    rSz->Add(tabBar,0,wxEXPAND);
    rSz->Add(pageIn,1,wxEXPAND); rSz->Add(pageOut,1,wxEXPAND);
    rSz->Add(pageAddr,1,wxEXPAND); rSz->Add(pageFilt,1,wxEXPAND); rSz->Add(pageCompose,1,wxEXPAND);
    rightP->SetSizer(rSz);
    cSz->Add(wazoo,0,wxEXPAND); cSz->Add(wSep,0,wxEXPAND); cSz->Add(rightP,1,wxEXPAND);
    content->SetSizer(cSz);
    rootSz->Add(content,1,wxEXPAND);

    auto *status=new StatusBar(root); status->SetTheme(kLight);
    int initUnread=(int)std::count_if(kInbox,kInbox+kNumInbox,[](const Email &e){return e.unread;});
    status->Set(wxString::Format("In  \xE2\x80\x94  %d messages, %d unread",kNumInbox,initUnread),"IMAP Idle","Connected to mail.example.com");
    rootSz->Add(status,0,wxEXPAND);
    root->SetSizer(rootSz);

    // State
    auto *isDark=new bool(false);
    auto *composeTabIdx=new int(-1);
    wxPanel *fixedPages[]={pageIn,pageOut,pageAddr,pageFilt};

    auto showPage=[=](int tabIdx) {
      pageIn->Hide(); pageOut->Hide(); pageAddr->Hide(); pageFilt->Hide(); pageCompose->Hide();
      if (tabIdx>=0&&tabIdx<4) fixedPages[tabIdx]->Show();
      else if (*composeTabIdx>=0&&tabIdx==*composeTabIdx) pageCompose->Show();
      rSz->Layout();
    };

    // Tab events
    tabBar->onSelect=[=](int idx) {
      showPage(idx);
      if (idx==0) { int u=(int)std::count_if(kInbox,kInbox+kNumInbox,[](const Email &e){return e.unread;});
        status->Set(wxString::Format("In  \xE2\x80\x94  %d messages, %d unread",kNumInbox,u),"IMAP Idle","Connected to mail.example.com"); }
      else if (idx==1) status->Set(wxString::Format("Out  \xE2\x80\x94  %d messages",kNumOutbox),"SMTP Ready","");
      else if (idx==2) status->Set(wxString::Format("Contacts  \xE2\x80\x94  %d people",kNumContacts),"","");
      else if (idx==3) status->Set(wxString::Format("Filters  \xE2\x80\x94  %d rules",kNumFilters),"","");
    };

    tabBar->onClose=[=](int idx) {
      if (idx==*composeTabIdx) {
        tabBar->RemoveTab(idx); *composeTabIdx=-1; pageCompose->Hide();
        int ns=std::min(tabBar->m_sel,(int)tabBar->m_tabs.size()-1);
        tabBar->SelectTab(ns); showPage(ns); if (tabBar->onSelect) tabBar->onSelect(ns);
      }
    };

    msgListIn->onSelect=[=](int idx) {
      Email &e=kInbox[idx]; msgHdrIn->SetEmail(&e); previewIn->SetHTML(ViewHTML(e));
      e.unread=false; msgListIn->Refresh();
      int u=(int)std::count_if(kInbox,kInbox+kNumInbox,[](const Email &e){return e.unread;});
      status->Set(wxString::Format("In  \xE2\x80\x94  %d messages, %d unread",kNumInbox,u),"IMAP Idle","Connected to mail.example.com");
    };
    msgListOut->onSelect=[=](int idx) {
      Email &e=kOutbox[idx]; msgHdrOut->SetEmail(&e); previewOut->SetHTML(ViewHTML(e));
      status->Set(wxString::Format("Out  \xE2\x80\x94  %d messages",kNumOutbox),"SMTP Ready","");
    };
    contactList->onSelect=[=](int idx) { contactDetail->SetContact(idx); };
    wazoo->onMailboxSelect=[=](int idx) {
      if (idx==0) { tabBar->SelectTab(0); showPage(0); tabBar->onSelect(0); }
      else if (idx==1) { tabBar->SelectTab(1); showPage(1); tabBar->onSelect(1); }
    };

    // Compose
    auto openCompose=[=](bool reply, bool replyAll, bool fwd, const wxString &toAddr="") {
      if (!toAddr.empty()) {
        compHdr->Reset(toAddr); composeW->SetHTML(NewHTML());
      } else if (reply||replyAll||fwd) {
        int tab=tabBar->m_sel; Email *e=nullptr;
        if (tab==0&&msgListIn->m_sel>=0&&msgListIn->m_sel<kNumInbox) e=&kInbox[msgListIn->m_sel];
        else if (tab==1&&msgListOut->m_sel>=0&&msgListOut->m_sel<kNumOutbox) e=&kOutbox[msgListOut->m_sel];
        if (!e) return;
        if (reply||replyAll) {
          wxString subj=e->subject; if (!wxString(subj).StartsWith("Re: ")) subj="Re: "+subj;
          compHdr->Reset(e->from,replyAll&&strlen(e->cc)>0?e->cc:(replyAll?e->to:""),"",subj,"",e->pri>=PRI_HIGH?"High":"Normal");
          composeW->SetHTML(ReplyHTML(*e));
        } else {
          wxString subj=e->subject; if (!wxString(subj).StartsWith("Fwd: ")) subj="Fwd: "+subj;
          compHdr->Reset("","","",subj,e->hasAttach?"forwarded attachments":"","Normal");
          composeW->SetHTML(ReplyHTML(*e));
        }
      } else { compHdr->Reset(); composeW->SetHTML(NewHTML()); }

      if (*composeTabIdx<0) {
        wxString lbl=reply?"\xE2\x86\xA9 Reply":(replyAll?"\xE2\x87\x8F Reply All":(fwd?"\xE2\x86\x92 Forward":"\xE2\x9C\x8F New Message"));
        *composeTabIdx=tabBar->AddTab(lbl,true);
      } else {
        wxString lbl=reply?"\xE2\x86\xA9 Reply":(replyAll?"\xE2\x87\x8F Reply All":(fwd?"\xE2\x86\x92 Forward":"\xE2\x9C\x8F New Message"));
        tabBar->m_tabs[*composeTabIdx].label=lbl; tabBar->Refresh();
      }
      tabBar->SelectTab(*composeTabIdx); showPage(*composeTabIdx);
      status->Set("Composing message...","","");
    };

    auto closeCompose=[=]() {
      if (*composeTabIdx>=0) {
        tabBar->RemoveTab(*composeTabIdx); *composeTabIdx=-1; pageCompose->Hide();
        int ns=std::min(tabBar->m_sel,(int)tabBar->m_tabs.size()-1);
        tabBar->SelectTab(ns); showPage(ns); if (tabBar->onSelect) tabBar->onSelect(ns);
      }
    };

    // Contact detail -> compose
    contactDetail->onComposeTo=[=](const wxString &email) {
      openCompose(false,false,false,email);
    };

    bNew->Bind(wxEVT_BUTTON,[=](wxCommandEvent&) { openCompose(false,false,false); });
    bReply->Bind(wxEVT_BUTTON,[=](wxCommandEvent&) { openCompose(true,false,false); });
    bReplyAll->Bind(wxEVT_BUTTON,[=](wxCommandEvent&) { openCompose(false,true,false); });
    bFwd->Bind(wxEVT_BUTTON,[=](wxCommandEvent&) { openCompose(false,false,true); });
    bDiscard->Bind(wxEVT_BUTTON,[=](wxCommandEvent&) { closeCompose(); });
    bSend->Bind(wxEVT_BUTTON,[=](wxCommandEvent&) { closeCompose(); wxMessageBox("Message sent!","Eudora 2026",wxOK|wxICON_INFORMATION,frame); });
    bQueue->Bind(wxEVT_BUTTON,[=](wxCommandEvent&) { closeCompose(); status->Set("Message queued in Out mailbox","SMTP Ready",""); });
    bBold->Bind(wxEVT_BUTTON,[composeW](wxCommandEvent&) { composeW->ToggleBold(); composeW->SetFocus(); });
    bItalic->Bind(wxEVT_BUTTON,[composeW](wxCommandEvent&) { composeW->ToggleItalic(); composeW->SetFocus(); });
    bUline->Bind(wxEVT_BUTTON,[composeW](wxCommandEvent&) { composeW->ToggleUnderline(); composeW->SetFocus(); });
    bStrike->Bind(wxEVT_BUTTON,[composeW](wxCommandEvent&) { composeW->ToggleStrikethrough(); composeW->SetFocus(); });
    auto *composeFontSize=new int(11);
    bFontUp->Bind(wxEVT_BUTTON,[composeW,composeFontSize](wxCommandEvent&) { if(*composeFontSize<36)*composeFontSize+=1; composeW->SetFontSize(*composeFontSize); composeW->SetFocus(); });
    bFontDn->Bind(wxEVT_BUTTON,[composeW,composeFontSize](wxCommandEvent&) { if(*composeFontSize>8)*composeFontSize-=1; composeW->SetFontSize(*composeFontSize); composeW->SetFocus(); });
    bColor->Bind(wxEVT_BUTTON,[composeW,frame](wxCommandEvent&) {
      wxColourData cd; cd.SetChooseFull(true);
      wxColourDialog dlg(frame,&cd);
      if (dlg.ShowModal()==wxID_OK) composeW->SetTextColor(dlg.GetColourData().GetColour());
      composeW->SetFocus();
    });
    bBullet->Bind(wxEVT_BUTTON,[composeW](wxCommandEvent&) { composeW->ToggleBulletList(); composeW->SetFocus(); });
    bNumber->Bind(wxEVT_BUTTON,[composeW](wxCommandEvent&) { composeW->ToggleOrderedList(); composeW->SetFocus(); });
    bQu->Bind(wxEVT_BUTTON,[composeW](wxCommandEvent&) { composeW->IncreaseQuoteLevel(); composeW->SetFocus(); });
    bUnqu->Bind(wxEVT_BUTTON,[composeW](wxCommandEvent&) { composeW->DecreaseQuoteLevel(); composeW->SetFocus(); });
    bHR->Bind(wxEVT_BUTTON,[composeW](wxCommandEvent&) { composeW->InsertHR(); composeW->SetFocus(); });
    bLink->Bind(wxEVT_BUTTON,[composeW,frame](wxCommandEvent&) {
      wxString url=wxGetTextFromUser("Enter URL:","Insert Link","https://",frame);
      if (!url.empty()) composeW->InsertLink(url,url);
      composeW->SetFocus();
    });
    bDel->Bind(wxEVT_BUTTON,[=](wxCommandEvent&) {
      int tab=tabBar->m_sel;
      if (tab==0&&msgListIn->m_sel>=0&&msgListIn->m_sel<kNumInbox)
        status->Set(wxString::Format("Moved \"%s\" to Trash",kInbox[msgListIn->m_sel].subject),"","");
    });
    bCheck->Bind(wxEVT_BUTTON,[=](wxCommandEvent&) { status->Set("Checking mail...","Connecting to mail.example.com",""); });

    // Dark mode
    bDark->Bind(wxEVT_BUTTON,[=](wxCommandEvent&) {
      *isDark=!*isDark; const Theme &t=*isDark?kDark:kLight;
      auto scheme=*isDark?wxHtmlEditWidget::ColorScheme::Dark:wxHtmlEditWidget::ColorScheme::Light;
      bDark->m_icon=*isDark?"\xE2\x98\x80":"\xE2\x98\xBD";
      root->SetBackgroundColour(t.toolbarBg); setTBTheme(t);
      bNew->m_accent=true; bNew->m_bg=wxColour(50,110,165); bNew->m_hoverBg=wxColour(40,95,148); bNew->m_pressedBg=wxColour(30,80,130);
      tbLine->SetBackgroundColour(t.toolbarBorder); wSep->SetBackgroundColour(t.toolbarBorder);
      addrSep->SetBackgroundColour(t.toolbarBorder);
      wazoo->SetTheme(t); tabBar->SetTheme(t);
      msgListIn->SetTheme(t); msgHdrIn->SetTheme(t); msgListOut->SetTheme(t); msgHdrOut->SetTheme(t);
      contactList->SetTheme(t); contactDetail->SetTheme(t); filterList->SetTheme(t);
      previewIn->GetEngine().bgColor=t.editorBg; previewIn->GetEngine().defaultColor=t.editorFg; previewIn->SetColorScheme(scheme);
      previewOut->GetEngine().bgColor=t.editorBg; previewOut->GetEngine().defaultColor=t.editorFg; previewOut->SetColorScheme(scheme);
      compHdr->SetTheme(t); pageCompose->SetBackgroundColour(t.headerBg); setCTTheme(t);
      ctLine->SetBackgroundColour(t.toolbarBorder);
      bSend->m_accent=true; bSend->m_bg=wxColour(50,110,165); bSend->m_hoverBg=wxColour(40,95,148); bSend->m_pressedBg=wxColour(30,80,130);
      composeW->GetEngine().bgColor=t.editorBg; composeW->GetEngine().defaultColor=t.editorFg; composeW->SetColorScheme(scheme);
      status->SetTheme(t); frame->Refresh();
    });

    frame->Show(); return true;
  }
};

wxIMPLEMENT_APP(EudoraApp);
