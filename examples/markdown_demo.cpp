#include "wxHtmlEditWidget.h"
#include "MarkdownParser.h"
#include <wx/wx.h>
#include <wx/splitter.h>

// ============================================================
// Markdown Editor Demo
// ============================================================
// Left pane: raw Markdown text (wxTextCtrl)
// Right pane: live rendered preview (wxHtmlEditWidget)
// Bottom bar: buttons to switch between Markdown source / HTML export

static const wxString SAMPLE_MARKDOWN =
    "# Markdown Editor\n"
    "\n"
    "This is a **live preview** of your Markdown content.\n"
    "\n"
    "## Features\n"
    "\n"
    "- **Bold**, *italic*, and ~~strikethrough~~\n"
    "- `Inline code` and code blocks\n"
    "- [Links](https://example.com) and images\n"
    "- Ordered and unordered lists\n"
    "- Tables with alignment\n"
    "- Blockquotes\n"
    "- Headings (ATX and Setext)\n"
    "\n"
    "## Code Block\n"
    "\n"
    "```cpp\n"
    "#include <iostream>\n"
    "\n"
    "int main() {\n"
    "    std::cout << \"Hello, Markdown!\" << std::endl;\n"
    "    return 0;\n"
    "}\n"
    "```\n"
    "\n"
    "## Table\n"
    "\n"
    "| Feature       | Status    | Notes          |\n"
    "|:--------------|:---------:|---------------:|\n"
    "| Parsing       | Done      | Round-trip     |\n"
    "| Serialization | Done      | Preserves style|\n"
    "| Editing       | Live      | Real-time      |\n"
    "\n"
    "## Blockquote\n"
    "\n"
    "> The best way to predict the future\n"
    "> is to invent it.\n"
    ">\n"
    "> — Alan Kay\n"
    "\n"
    "---\n"
    "\n"
    "### Ordered List\n"
    "\n"
    "1. First item\n"
    "2. Second item\n"
    "3. Third item\n"
    "\n"
    "Try editing the Markdown on the left!\n";

class MarkdownDemoApp : public wxApp {
public:
  bool OnInit() override {
    wxInitAllImageHandlers();

    auto *frame = new wxFrame(nullptr, wxID_ANY, "Markdown Editor",
                              wxDefaultPosition, wxSize(1100, 750));

    // Main vertical sizer
    auto *mainSizer = new wxBoxSizer(wxVERTICAL);

    // Splitter for editor / preview
    auto *splitter = new wxSplitterWindow(frame, wxID_ANY,
                                          wxDefaultPosition, wxDefaultSize,
                                          wxSP_3D | wxSP_LIVE_UPDATE);

    // Left pane: Markdown source editor
    auto *editor = new wxTextCtrl(splitter, wxID_ANY, SAMPLE_MARKDOWN,
                                  wxDefaultPosition, wxDefaultSize,
                                  wxTE_MULTILINE | wxTE_RICH2 | wxHSCROLL);
    editor->SetFont(wxFont(13, wxFONTFAMILY_TELETYPE, wxFONTSTYLE_NORMAL,
                           wxFONTWEIGHT_NORMAL));

    // Right pane: rendered preview
    auto *preview = new wxHtmlEditWidget(splitter, wxID_ANY);
    preview->SetReadOnly(true);

    // Dark theme CSS for the preview
    preview->AddStyle(
        "body { font-family: -apple-system, system-ui, Helvetica, Arial, "
        "sans-serif; background: #1a1a2e; color: #e0e0e0; padding: 16px; "
        "line-height: 1.6; }"
        "h1, h2, h3 { color: #60a5fa; }"
        "h1 { font-size: 22pt; border-bottom: 1px solid #334155; "
        "padding-bottom: 8px; }"
        "h2 { font-size: 17pt; }"
        "h3 { font-size: 14pt; }"
        "code { font-family: 'SF Mono', Menlo, Consolas, monospace; "
        "background: #2d2d44; padding: 2px 5px; border-radius: 3px; "
        "color: #f59e0b; font-size: 10pt; }"
        "pre { background: #16213e; padding: 12px; border-radius: 6px; "
        "border: 1px solid #334155; overflow: auto; }"
        "pre code { background: none; padding: 0; color: #e0e0e0; }"
        "blockquote { border-left: 3px solid #60a5fa; margin: 12px 0; "
        "padding: 8px 16px; background: #16213e; color: #94a3b8; }"
        "table { border-collapse: collapse; width: 100%; margin: 12px 0; }"
        "th { background: #16213e; color: #60a5fa; font-weight: bold; "
        "padding: 8px 12px; border: 1px solid #334155; }"
        "td { padding: 8px 12px; border: 1px solid #334155; }"
        "tr:nth-child(even) td { background: #16213e; }"
        "hr { border: none; border-top: 1px solid #334155; margin: 16px 0; }"
        "a { color: #60a5fa; }"
        "ul, ol { padding-left: 24px; }"
        "li { margin: 4px 0; }",
        true);

    // Initial render
    preview->SetMarkdown(SAMPLE_MARKDOWN);

    splitter->SplitVertically(editor, preview);
    splitter->SetMinimumPaneSize(200);
    splitter->SetSashGravity(0.45);

    mainSizer->Add(splitter, 1, wxEXPAND);

    // Bottom toolbar
    auto *toolbar = new wxPanel(frame, wxID_ANY);
    toolbar->SetBackgroundColour(wxColour(30, 30, 46));
    auto *tbSizer = new wxBoxSizer(wxHORIZONTAL);

    auto *lblStatus = new wxStaticText(toolbar, wxID_ANY, " Markdown Editor");
    lblStatus->SetForegroundColour(wxColour(150, 150, 180));
    lblStatus->SetFont(wxFont(10, wxFONTFAMILY_DEFAULT, wxFONTSTYLE_NORMAL,
                              wxFONTWEIGHT_BOLD));
    tbSizer->Add(lblStatus, 0, wxALIGN_CENTER_VERTICAL | wxLEFT, 8);

    tbSizer->AddStretchSpacer();

    auto *btnGetMd = new wxButton(toolbar, wxID_ANY, "Get Markdown");
    auto *btnGetHtml = new wxButton(toolbar, wxID_ANY, "Get HTML");
    auto *btnRoundTrip = new wxButton(toolbar, wxID_ANY, "Round-Trip Test");
    tbSizer->Add(btnGetMd, 0, wxALL | wxALIGN_CENTER_VERTICAL, 4);
    tbSizer->Add(btnGetHtml, 0, wxALL | wxALIGN_CENTER_VERTICAL, 4);
    tbSizer->Add(btnRoundTrip, 0, wxALL | wxALIGN_CENTER_VERTICAL, 4);

    toolbar->SetSizer(tbSizer);
    mainSizer->Add(toolbar, 0, wxEXPAND);

    frame->SetSizer(mainSizer);

    // --- Live preview: update on every keystroke ---
    editor->Bind(wxEVT_TEXT, [=](wxCommandEvent &) {
      wxString md = editor->GetValue();
      preview->SetMarkdown(md);
    });

    // --- Get Markdown: show round-tripped markdown in a dialog ---
    btnGetMd->Bind(wxEVT_BUTTON, [=](wxCommandEvent &) {
      wxString md = preview->GetMarkdown();
      wxDialog dlg(frame, wxID_ANY, "Markdown Output",
                   wxDefaultPosition, wxSize(600, 500));
      auto *sizer = new wxBoxSizer(wxVERTICAL);
      auto *txt = new wxTextCtrl(&dlg, wxID_ANY, md,
                                 wxDefaultPosition, wxDefaultSize,
                                 wxTE_MULTILINE | wxTE_READONLY | wxHSCROLL);
      txt->SetFont(wxFont(12, wxFONTFAMILY_TELETYPE, wxFONTSTYLE_NORMAL,
                          wxFONTWEIGHT_NORMAL));
      sizer->Add(txt, 1, wxEXPAND | wxALL, 8);
      dlg.SetSizer(sizer);
      dlg.ShowModal();
    });

    // --- Get HTML: show HTML export in a dialog ---
    btnGetHtml->Bind(wxEVT_BUTTON, [=](wxCommandEvent &) {
      wxString html = preview->GetHTML();
      wxDialog dlg(frame, wxID_ANY, "HTML Output",
                   wxDefaultPosition, wxSize(600, 500));
      auto *sizer = new wxBoxSizer(wxVERTICAL);
      auto *txt = new wxTextCtrl(&dlg, wxID_ANY, html,
                                 wxDefaultPosition, wxDefaultSize,
                                 wxTE_MULTILINE | wxTE_READONLY | wxHSCROLL);
      txt->SetFont(wxFont(12, wxFONTFAMILY_TELETYPE, wxFONTSTYLE_NORMAL,
                          wxFONTWEIGHT_NORMAL));
      sizer->Add(txt, 1, wxEXPAND | wxALL, 8);
      dlg.SetSizer(sizer);
      dlg.ShowModal();
    });

    // --- Round-trip test: MD → tree → MD, then reload ---
    btnRoundTrip->Bind(wxEVT_BUTTON, [=](wxCommandEvent &) {
      wxString original = editor->GetValue();
      // Parse → serialize → parse again
      Document doc1 = ParseMarkdown(original);
      wxString md1 = SerializeMarkdown(doc1);
      Document doc2 = ParseMarkdown(md1);
      wxString md2 = SerializeMarkdown(doc2);

      // Show comparison
      wxDialog dlg(frame, wxID_ANY, "Round-Trip Test",
                   wxDefaultPosition, wxSize(800, 600));
      auto *sizer = new wxBoxSizer(wxVERTICAL);

      bool match = (md1 == md2);
      auto *lbl = new wxStaticText(&dlg, wxID_ANY,
          match ? "Round-trip STABLE (2nd pass matches 1st)"
                : "Round-trip DIVERGED (2nd pass differs from 1st)");
      lbl->SetForegroundColour(match ? wxColour(0, 180, 0) : wxColour(220, 60, 60));
      lbl->SetFont(wxFont(12, wxFONTFAMILY_DEFAULT, wxFONTSTYLE_NORMAL,
                          wxFONTWEIGHT_BOLD));
      sizer->Add(lbl, 0, wxALL, 8);

      auto *split = new wxSplitterWindow(&dlg, wxID_ANY);
      auto *txt1 = new wxTextCtrl(split, wxID_ANY, md1,
                                  wxDefaultPosition, wxDefaultSize,
                                  wxTE_MULTILINE | wxTE_READONLY | wxHSCROLL);
      auto *txt2 = new wxTextCtrl(split, wxID_ANY, md2,
                                  wxDefaultPosition, wxDefaultSize,
                                  wxTE_MULTILINE | wxTE_READONLY | wxHSCROLL);
      txt1->SetFont(wxFont(11, wxFONTFAMILY_TELETYPE, wxFONTSTYLE_NORMAL,
                           wxFONTWEIGHT_NORMAL));
      txt2->SetFont(wxFont(11, wxFONTFAMILY_TELETYPE, wxFONTSTYLE_NORMAL,
                           wxFONTWEIGHT_NORMAL));
      split->SplitVertically(txt1, txt2);
      split->SetSashGravity(0.5);

      sizer->Add(split, 1, wxEXPAND | wxALL, 8);
      dlg.SetSizer(sizer);
      dlg.ShowModal();
    });

    frame->Show(true);
    return true;
  }
};

wxIMPLEMENT_APP(MarkdownDemoApp);
