#include "wxHtmlEditWidget.h"
#include <wx/wx.h>
#include <wx/spinctrl.h>

static const wxString DEMO_HTML =
    "<body style=\"font-family: Georgia, serif; font-size: 12pt; "
    "color: #333; padding: 20px;\">"

    "<h1 style=\"color: #2c3e50; border-bottom: 2px solid #2c3e50; "
    "padding-bottom: 8px;\">wxHtmlEdit Print Demo</h1>"

    "<p>This document demonstrates the <b>printing</b> and "
    "<b>print preview</b> capabilities of wxHtmlEditWidget. "
    "Click the buttons below to try them out.</p>"

    "<h2 style=\"color: #8e44ad;\">Feature List</h2>"
    "<ul>"
    "  <li><b>Configurable margins</b> &mdash; top, bottom, left, right (in mm)</li>"
    "  <li><b>Page headers</b> &mdash; optional document title centered at top</li>"
    "  <li><b>Page footers</b> &mdash; automatic &ldquo;Page X of Y&rdquo; numbering</li>"
    "  <li><b>Print Preview</b> &mdash; paginated on-screen preview before printing</li>"
    "  <li><b>Multi-page support</b> &mdash; documents automatically paginated</li>"
    "  <li><b>Smart page breaks</b> &mdash; CSS page-break, orphans, widows support</li>"
    "  <li><b>Thead repetition</b> &mdash; table headers repeat on every page</li>"
    "  <li><b>Shrink-to-fit</b> &mdash; wide content auto-scaled to page</li>"
    "  <li><b>@media print</b> &mdash; print-specific CSS rules</li>"
    "</ul>"

    // Use <style> with @media print to hide certain things in print
    "<style>@media print { .no-print { display: none; } }</style>"
    "<p class=\"no-print\" style=\"color: #e67e22; font-style: italic;\">"
    "This paragraph is hidden in print (@media print { .no-print { display: none; } })</p>"

    "<h2 style=\"color: #e74c3c;\">Sample Table (with thead)</h2>"
    "<table style=\"width: 100%; border-collapse: collapse; margin-bottom: 16px;\">"
    "  <thead>"
    "  <tr style=\"background: #2c3e50; color: white;\">"
    "    <th style=\"padding: 8px; border: 1px solid #1a252f;\">Setting</th>"
    "    <th style=\"padding: 8px; border: 1px solid #1a252f;\">Default</th>"
    "    <th style=\"padding: 8px; border: 1px solid #1a252f;\">Description</th>"
    "  </tr>"
    "  </thead>"
    "  <tbody>"
    "  <tr style=\"background: #ecf0f1;\">"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">marginTop</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">20 mm</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Top page margin</td>"
    "  </tr>"
    "  <tr>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">marginBottom</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">20 mm</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Bottom page margin</td>"
    "  </tr>"
    "  <tr style=\"background: #ecf0f1;\">"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">marginLeft</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">15 mm</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Left page margin</td>"
    "  </tr>"
    "  <tr>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">marginRight</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">15 mm</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Right page margin</td>"
    "  </tr>"
    "  <tr style=\"background: #ecf0f1;\">"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">showHeader</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">false</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Show title in page header</td>"
    "  </tr>"
    "  <tr>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">showFooter</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">true</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Show page numbers in footer</td>"
    "  </tr>"
    "  <tr style=\"background: #ecf0f1;\">"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">scale</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">1.0</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Print scale factor</td>"
    "  </tr>"
    "  <tr>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">paperSize</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">A4</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Paper size (A4, Letter, Legal, A3, A5)</td>"
    "  </tr>"
    "  <tr style=\"background: #ecf0f1;\">"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">orientation</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Portrait</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Page orientation</td>"
    "  </tr>"
    "  <tr>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">printBackgrounds</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">true</td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">Print background colors</td>"
    "  </tr>"
    "  </tbody>"
    "</table>"

    "<h2 style=\"color: #27ae60;\">Code Example</h2>"
    "<pre style=\"background: #f4f4f4; padding: 12px; border: 1px solid #ddd; "
    "font-family: monospace; font-size: 10pt;\">"
    "// Quick print with defaults\n"
    "widget-&gt;Print();\n"
    "\n"
    "// Print preview with settings dialog\n"
    "widget-&gt;PrintPreviewWithDialog();\n"
    "\n"
    "// Manual settings\n"
    "wxHtmlEditWidget::PrintSettings ps;\n"
    "ps.paperSize = wxHtmlEditWidget::PaperSize::Letter;\n"
    "ps.orientation = wxHtmlEditWidget::Orientation::Landscape;\n"
    "ps.scale = 0.75;\n"
    "ps.printBackgrounds = false;\n"
    "widget-&gt;PrintPreview(ps);\n"
    "</pre>"

    "<h2 style=\"color: #2980b9;\">Filler Text</h2>"
    "<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod "
    "tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, "
    "quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo "
    "consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse "
    "cillum dolore eu fugiat nulla pariatur.</p>"

    "<p>Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia "
    "deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste "
    "natus error sit voluptatem accusantium doloremque laudantium, totam rem "
    "aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto "
    "beatae vitae dicta sunt explicabo.</p>"

    "<p>Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, "
    "sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. "
    "Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, "
    "adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et "
    "dolore magnam aliquam quaerat voluptatem.</p>"

    "<p style=\"color: #999; font-size: 10pt; margin-top: 24px; "
    "border-top: 1px solid #ddd; padding-top: 8px;\">"
    "Powered by wxHtmlEdit &mdash; printing demo</p>"
    "</body>";

class PrintDemoApp : public wxApp {
public:
  bool OnInit() override {
    wxInitAllImageHandlers();
    auto *frame = new wxFrame(nullptr, wxID_ANY, "wxHtmlEdit - Print Demo",
                              wxDefaultPosition, wxSize(900, 700));

    auto *panel = new wxPanel(frame);
    auto *vbox = new wxBoxSizer(wxVERTICAL);

    // Toolbar
    auto *tbPanel = new wxPanel(panel);
    tbPanel->SetBackgroundColour(wxColour(245, 245, 245));
    auto *tbSizer = new wxBoxSizer(wxHORIZONTAL);

    auto *btnPrint = new wxButton(tbPanel, wxID_ANY, "Print...");
    btnPrint->SetToolTip("Print document (Ctrl+P)");
    auto *btnPreview = new wxButton(tbPanel, wxID_ANY, "Print Preview");
    btnPreview->SetToolTip("Quick preview with current settings");
    auto *btnPreviewDlg = new wxButton(tbPanel, wxID_ANY, "Preview (Settings)...");
    btnPreviewDlg->SetToolTip("Configure settings then preview");

    auto *chkHeader = new wxCheckBox(tbPanel, wxID_ANY, "Header");
    chkHeader->SetToolTip("Show document title in header");
    auto *chkFooter = new wxCheckBox(tbPanel, wxID_ANY, "Footer");
    chkFooter->SetValue(true);
    chkFooter->SetToolTip("Show page numbers in footer");

    auto *lblScale = new wxStaticText(tbPanel, wxID_ANY, "Scale:");
    auto *spinScale = new wxSpinCtrl(tbPanel, wxID_ANY, "100",
                                      wxDefaultPosition, wxSize(70, -1),
                                      wxSP_ARROW_KEYS, 25, 400, 100);
    spinScale->SetToolTip("Print scale (100% = fit page width)");

    tbSizer->Add(btnPrint, 0, wxALL | wxALIGN_CENTER_VERTICAL, 4);
    tbSizer->Add(btnPreview, 0, wxALL | wxALIGN_CENTER_VERTICAL, 4);
    tbSizer->Add(btnPreviewDlg, 0, wxALL | wxALIGN_CENTER_VERTICAL, 4);
    tbSizer->AddSpacer(16);
    tbSizer->Add(chkHeader, 0, wxALL | wxALIGN_CENTER_VERTICAL, 4);
    tbSizer->Add(chkFooter, 0, wxALL | wxALIGN_CENTER_VERTICAL, 4);
    tbSizer->AddSpacer(8);
    tbSizer->Add(lblScale, 0, wxALL | wxALIGN_CENTER_VERTICAL, 4);
    tbSizer->Add(spinScale, 0, wxALL | wxALIGN_CENTER_VERTICAL, 4);

    tbPanel->SetSizer(tbSizer);
    vbox->Add(tbPanel, 0, wxEXPAND);

    // Editor widget
    auto *widget = new wxHtmlEditWidget(panel);
    widget->SetReadOnly(true);
    widget->SetHTML(DEMO_HTML);
    vbox->Add(widget, 1, wxEXPAND);

    panel->SetSizer(vbox);

    // Build settings from toolbar state
    auto makeSettings = [=]() {
      wxHtmlEditWidget::PrintSettings ps;
      ps.showHeader = chkHeader->GetValue();
      ps.showFooter = chkFooter->GetValue();
      ps.title = "wxHtmlEdit Print Demo";
      ps.scale = spinScale->GetValue() / 100.0;
      return ps;
    };

    btnPrint->Bind(wxEVT_BUTTON, [=](wxCommandEvent &) {
      widget->Print(makeSettings(), frame);
    });

    btnPreview->Bind(wxEVT_BUTTON, [=](wxCommandEvent &) {
      widget->PrintPreview(makeSettings(), frame);
    });

    btnPreviewDlg->Bind(wxEVT_BUTTON, [=](wxCommandEvent &) {
      widget->PrintPreviewWithDialog(frame);
    });

    // Keyboard shortcut: Ctrl+P
    frame->Bind(wxEVT_CHAR_HOOK, [=](wxKeyEvent &evt) {
      if (evt.GetModifiers() == wxMOD_CONTROL && evt.GetKeyCode() == 'P') {
        widget->PrintPreviewWithDialog(frame);
      } else {
        evt.Skip();
      }
    });

    frame->Show();
    return true;
  }
};

wxIMPLEMENT_APP(PrintDemoApp);
