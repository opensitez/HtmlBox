// Diagnostic: dump box tree for the sidebar HTML from graph_demo
// to see exactly what inline-block + float layout produces.
#include "HtmlParser.h"
#include "LayoutEngine.h"
#include "Types.h"
#include <wx/wx.h>
#include <wx/dcmemory.h>
#include <wx/image.h>
#include <cstdio>

static void DumpBoxTree(const Box &b, int depth = 0) {
  if (b.style.display == Display::None) return;
  for (int i = 0; i < depth; i++) printf("  ");

  const char *dispStr = "?";
  switch (b.style.display) {
    case Display::Block: dispStr = "block"; break;
    case Display::Inline: dispStr = "inline"; break;
    case Display::InlineBlock: dispStr = "inline-block"; break;
    case Display::Flex: dispStr = "flex"; break;
    case Display::None: dispStr = "none"; break;
    default: dispStr = "other"; break;
  }
  const char *fltStr = "";
  if (b.style.cssFloat == Float::Left) fltStr = " float:left";
  if (b.style.cssFloat == Float::Right) fltStr = " float:right";

  printf("%-8s disp=%-12s anon=%d%s  content=[%d,%d %dx%d]  margin=[%d,%d %dx%d]",
         b.tag.empty() ? (b.isAnonymous ? "(anon)" : "(box)") : b.tag.c_str(),
         dispStr, b.isAnonymous, fltStr,
         b.contentRect.x, b.contentRect.y,
         b.contentRect.width, b.contentRect.height,
         b.marginRect.x, b.marginRect.y,
         b.marginRect.width, b.marginRect.height);

  if (!b.inlineContent.empty()) {
    printf("  runs=%zu[", b.inlineContent.size());
    for (size_t i = 0; i < b.inlineContent.size(); i++) {
      auto &r = b.inlineContent[i];
      if (r.atomicBox) {
        const char *aDisp = "?";
        switch (r.atomicBox->style.display) {
          case Display::Block: aDisp = "block"; break;
          case Display::InlineBlock: aDisp = "inline-block"; break;
          case Display::Inline: aDisp = "inline"; break;
          default: aDisp = "other"; break;
        }
        const char *aFlt = "";
        if (r.atomicBox->style.cssFloat == Float::Left) aFlt = ",fL";
        if (r.atomicBox->style.cssFloat == Float::Right) aFlt = ",fR";
        printf("ATOM(%s,%s%s,p=%p)", r.atomicBox->tag.c_str(), aDisp, aFlt, (void*)r.atomicBox);
      } else {
        wxString s = b.ownText.Mid(r.textOffset, std::min(r.length, (size_t)30));
        printf("\"%s\"", (const char *)s.utf8_str());
      }
      if (i + 1 < b.inlineContent.size()) printf(", ");
    }
    printf("]");
  }
  if (!b.id.empty()) printf("  #%s", b.id.c_str());
  if (!b.className.empty()) printf("  .%s", b.className.c_str());
  printf("\n");
  for (auto &ch : b.children) DumpBoxTree(*ch, depth + 1);
}

class DiagApp : public wxApp {
public:
  bool OnInit() override {
    wxInitAllImageHandlers();

    // Exact sidebar HTML from graph_demo, inside flex layout
    wxString html =
      "<html><head><style>"
      ".main { display: flex; }"
      ".sidebar {"
      "  background: #161b22; border-right: 1px solid #30363d;"
      "  padding: 14px; width: 170px; min-width: 170px;"
      "}"
      ".sb-item {"
      "  padding: 5px 8px; margin-bottom: 2px; border-radius: 6px;"
      "  font-size: 8pt; color: #c9d1d9;"
      "  border: 1px solid transparent;"
      "}"
      ".sb-item:hover { background: #21262d; border-color: #30363d; }"
      ".sb-item .sstat { color: #8b949e; float: right; }"
      ".sb-dot {"
      "  display: inline-block; width: 6px; height: 6px;"
      "  border-radius: 3px; margin-right: 5px;"
      "}"
      "</style></head>"
      "<body>"
      "<div class='main'>"
      "<div class='sidebar'>"

      "<div class='sb-item' id='sb-organic'>"
      "  <span class='sb-dot' style='background:#4e79a7;'></span>Organic <span class='sstat'>48%</span>"
      "</div>"

      "<div class='sb-item' id='sb-direct'>"
      "  <span class='sb-dot' style='background:#f28e2b;'></span>Direct <span class='sstat'>27%</span>"
      "</div>"

      "</div>"
      "<div class='content' style='flex:1;'>Content</div>"
      "</div>"
      "</body></html>";

    Document doc = ParseHTML(html);
    printf("=== AFTER PARSE (before stylesheet) ===\n");
    DumpBoxTree(*doc.root);

    wxBitmap bmp(400, 400);
    wxMemoryDC dc(bmp);
    LayoutEngine engine;

    engine.ApplyStylesheet(doc);
    printf("\n=== AFTER ApplyStylesheet (flattening done) ===\n");
    DumpBoxTree(*doc.root);

    engine.Layout(dc, doc, 170);
    printf("\n=== AFTER Layout (170px viewport) ===\n");
    DumpBoxTree(*doc.root);

    // Re-layout to check stability
    engine.Layout(dc, doc, 170);
    printf("\n=== AFTER 2nd Layout (stability check) ===\n");
    DumpBoxTree(*doc.root);

    // ---- Hover simulation ----
    // Find boxes by id
    std::function<const Box*(const Box&, const std::string&)> findById =
      [&findById](const Box &root, const std::string &id) -> const Box* {
      if (root.id == id) return &root;
      for (auto &ch : root.children) {
        auto *f = findById(*ch, id);
        if (f) return f;
      }
      return nullptr;
    };

    const Box *organic = findById(*doc.root, "sb-organic");
    const Box *direct = findById(*doc.root, "sb-direct");
    printf("\norganic=%p direct=%p\n", (void*)organic, (void*)direct);

    // Simulate hover on organic
    engine.linkState.hoveredBox = organic;
    engine.ApplyStylesheetWithState(doc);
    engine.Layout(dc, doc, 170);
    printf("\n=== HOVER on organic ===\n");
    DumpBoxTree(*doc.root);
    // Check organic style
    organic = findById(*doc.root, "sb-organic");
    if (organic) {
      printf("organic bg=%s border-top-color=%s\n",
             organic->style.backgroundColor.IsOk() ?
               (const char*)organic->style.backgroundColor.GetAsString().utf8_str() : "(none)",
             organic->style.border.top.color.IsOk() ?
               (const char*)organic->style.border.top.color.GetAsString().utf8_str() : "(none)");
    }

    // Now hover on direct
    direct = findById(*doc.root, "sb-direct");
    engine.linkState.hoveredBox = direct;
    engine.ApplyStylesheetWithState(doc);
    engine.Layout(dc, doc, 170);
    printf("\n=== HOVER on direct ===\n");
    DumpBoxTree(*doc.root);
    organic = findById(*doc.root, "sb-organic");
    direct = findById(*doc.root, "sb-direct");
    if (organic) {
      printf("organic bg=%s border-top-color=%s\n",
             organic->style.backgroundColor.IsOk() ?
               (const char*)organic->style.backgroundColor.GetAsString().utf8_str() : "(none)",
             organic->style.border.top.color.IsOk() ?
               (const char*)organic->style.border.top.color.GetAsString().utf8_str() : "(none)");
    }
    if (direct) {
      printf("direct bg=%s border-top-color=%s\n",
             direct->style.backgroundColor.IsOk() ?
               (const char*)direct->style.backgroundColor.GetAsString().utf8_str() : "(none)",
             direct->style.border.top.color.IsOk() ?
               (const char*)direct->style.border.top.color.GetAsString().utf8_str() : "(none)");
    }

    exit(0);
    return true;
  }
};

wxIMPLEMENT_APP(DiagApp);
