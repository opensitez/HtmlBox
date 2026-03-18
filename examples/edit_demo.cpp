#include "wxHtmlEditWidget.h"
#include <wx/wx.h>
#include <wx/clipbrd.h>
#include <wx/clrpicker.h>
#include <wx/filename.h>
#include <wx/stdpaths.h>
#include <wx/wrapsizer.h>
#include <wx/statline.h>

static const wxString EDIT_HTML =
    "<body style=\"font-family: Georgia, serif; font-size: 12pt; "
    "padding: 16px;\">"

    "<h1 style=\"color: #2c3e50;\">wxHtmlEdit Layout Test</h1>"

    // --- Flexbox row ---
    "<h2 style=\"color: #8e44ad;\">Flexbox Row</h2>"
    "<div style=\"display: flex; gap: 12px; margin-bottom: 16px;\">"
    "  <div style=\"flex: 1; background: #eaf2f8; padding: 10px; "
    "border: 1px solid #aed6f1;\">"
    "    <b>Column 1</b><br/>Flex item with <em>flex:1</em>. "
    "Edit this text freely.</div>"
    "  <div style=\"flex: 2; background: #fef9e7; padding: 10px; "
    "border: 1px solid #f9e79f;\">"
    "    <b>Column 2</b><br/>This column has <em>flex:2</em>, "
    "so it takes twice the space. "
    "It contains <u>underlined</u> and <b>bold</b> words.</div>"
    "  <div style=\"flex: 1; background: #fdedec; padding: 10px; "
    "border: 1px solid #f5b7b1;\">"
    "    <b>Column 3</b><br/>A shorter flex item.</div>"
    "</div>"

    // --- Table ---
    "<h2 style=\"color: #e74c3c;\">Table Layout</h2>"
    "<table style=\"width: 100%; border-collapse: collapse; "
    "margin-bottom: 16px;\">"
    "  <tr style=\"background: #2c3e50; color: white;\">"
    "    <th style=\"padding: 8px; border: 1px solid #1a252f;\">Name</th>"
    "    <th style=\"padding: 8px; border: 1px solid #1a252f;\">Type</th>"
    "    <th style=\"padding: 8px; border: 1px solid #1a252f;\">Description</th>"
    "  </tr>"
    "  <tr style=\"background: #ecf0f1;\">"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">"
    "<b>SetHTML</b></td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">"
    "<em>method</em></td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">"
    "Load an HTML string into the editor</td>"
    "  </tr>"
    "  <tr>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">"
    "<b>ToggleBold</b></td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">"
    "<em>method</em></td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">"
    "Toggle bold on current selection</td>"
    "  </tr>"
    "  <tr style=\"background: #ecf0f1;\">"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">"
    "<b>SetReadOnly</b></td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">"
    "<em>method</em></td>"
    "    <td style=\"padding: 8px; border: 1px solid #bdc3c7;\">"
    "Enable or disable editing mode</td>"
    "  </tr>"
    "</table>"

    // --- Grid ---
    "<h2 style=\"color: #27ae60;\">CSS Grid</h2>"
    "<div style=\"display: grid; grid-template-columns: 1fr 1fr 1fr; "
    "gap: 10px; margin-bottom: 16px;\">"
    "  <div style=\"background: #d5f5e3; padding: 10px; "
    "border: 1px solid #82e0aa;\">Grid cell <b>A1</b></div>"
    "  <div style=\"background: #d4efdf; padding: 10px; "
    "border: 1px solid #7dcea0;\">Grid cell <b>A2</b></div>"
    "  <div style=\"background: #d1f2eb; padding: 10px; "
    "border: 1px solid #76d7c4;\">Grid cell <b>A3</b></div>"
    "  <div style=\"background: #a9dfbf; padding: 10px; "
    "border: 1px solid #52be80; grid-column: span 2;\">"
    "This cell <em>spans two columns</em> using grid-column: span 2</div>"
    "  <div style=\"background: #a3e4d7; padding: 10px; "
    "border: 1px solid #48c9b0;\">Grid cell <b>B3</b></div>"
    "</div>"

    // --- Positioned div ---
    "<h2 style=\"color: #d35400;\">Positioned Elements</h2>"
    "<div style=\"position: relative; height: 150px; background: #fbeee6; "
    "border: 2px dashed #e59866; margin-bottom: 16px;\">"
    "  <div style=\"position: absolute; top: 10px; left: 10px; "
    "background: #f5cba7; padding: 8px; border: 1px solid #eb984e;\">"
    "Top-left (absolute)</div>"
    "  <div style=\"position: absolute; top: 10px; right: 10px; "
    "background: #fadbd8; padding: 8px; border: 1px solid #ec7063;\">"
    "Top-right (absolute)</div>"
    "  <div style=\"position: absolute; bottom: 10px; left: 50%; "
    "transform: translateX(-50%); "
    "background: #d2b4de; padding: 8px; border: 1px solid #a569bd;\">"
    "Bottom-center (absolute)</div>"
    "</div>"

    // --- Nested flex ---
    "<h2 style=\"color: #2980b9;\">Nested Flex</h2>"
    "<div style=\"display: flex; gap: 8px; margin-bottom: 16px;\">"
    "  <div style=\"flex: 1; display: flex; flex-direction: column; gap: 8px;\">"
    "    <div style=\"background: #d6eaf8; padding: 8px; "
    "border: 1px solid #85c1e9;\">Nested top</div>"
    "    <div style=\"background: #aed6f1; padding: 8px; "
    "border: 1px solid #5dade2;\">Nested bottom</div>"
    "  </div>"
    "  <div style=\"flex: 2; background: #ebf5fb; padding: 12px; "
    "border: 1px solid #85c1e9;\">"
    "    <p>This is the <b>main content area</b> in a nested flex layout. "
    "The left column is itself a flex column with two stacked items.</p>"
    "    <p>Try <em>selecting text across</em> these flex boundaries "
    "to test cross-container selection.</p>"
    "  </div>"
    "</div>"

    // --- Inline formatting showcase ---
    "<h2 style=\"color: #1abc9c;\">Inline Formatting</h2>"
    "<p>Normal, <b>bold</b>, <em>italic</em>, <u>underline</u>, "
    "<b><em>bold-italic</em></b>, "
    "<span style=\"color: #e74c3c;\">red text</span>, "
    "<span style=\"color: #3498db; font-size: 16pt;\">large blue</span>, "
    "<span style=\"background: #f1c40f; padding: 2px 4px;\">highlighted</span>, "
    "and <span style=\"font-family: monospace; background: #ecf0f1; "
    "padding: 2px 4px;\">monospace code</span>.</p>"

    "<p style=\"color: #999; font-size: 10pt; margin-top: 20px;\">"
    "Powered by wxHtmlEdit &mdash; layout engine test document.</p>"
    "</body>";

// Custom IDs
enum {
  ID_BOLD = wxID_HIGHEST + 1,
  ID_ITALIC,
  ID_UNDERLINE,
  ID_ALIGN_LEFT,
  ID_ALIGN_CENTER,
  ID_ALIGN_RIGHT,
  ID_ALIGN_JUSTIFY,
  ID_BULLET,
  ID_INDENT_LEFT,
  ID_INDENT_RIGHT,
  ID_HR,
  ID_IMAGE,
  ID_LINK,
  ID_TABLE,
  ID_QUOTE,
  ID_UNQUOTE,
  ID_CLEAR_FMT,
  ID_FONT_FAMILY,
  ID_FONT_SIZE,
  ID_FG_COLOR,
  ID_BG_COLOR,
  ID_DARK_MODE,
};

static wxString GetResDir() {
  wxFileName exePath(wxStandardPaths::Get().GetExecutablePath());
  wxString resDir = exePath.GetPath() + "/../res";
  if (!wxDirExists(resDir))
    resDir = exePath.GetPath() + "/../../res";
  return resDir;
}

static wxBitmap LoadIcon(const wxString &resDir, const wxString &name,
                         int size = 22) {
  wxString path = resDir + "/" + name + ".png";
  wxImage img;
  if (img.LoadFile(path, wxBITMAP_TYPE_PNG)) {
    img.Rescale(size, size, wxIMAGE_QUALITY_BILINEAR);
    return wxBitmap(img);
  }
  return wxBitmap(size, size);
}

static wxBitmapButton *IconBtn(wxWindow *parent, int id,
                                const wxString &resDir,
                                const wxString &iconName,
                                const wxString &tooltip) {
  auto *btn = new wxBitmapButton(parent, id, LoadIcon(resDir, iconName),
                                  wxDefaultPosition, wxSize(30, 28));
  btn->SetToolTip(tooltip);
  return btn;
}

static wxButton *LabelBtn(wxWindow *parent, int id, const wxString &label,
                            const wxString &tooltip) {
  auto *btn = new wxButton(parent, id, label, wxDefaultPosition, wxSize(-1, 28));
  btn->SetToolTip(tooltip);
  return btn;
}

class EditDemoApp : public wxApp {
public:
  bool OnInit() override {
    wxInitAllImageHandlers();
    auto *frame = new wxFrame(nullptr, wxID_ANY, "wxHtmlEdit - Editor Demo",
                              wxDefaultPosition, wxSize(1000, 780));

    auto *mainPanel = new wxPanel(frame);
    auto *vbox = new wxBoxSizer(wxVERTICAL);

    // --- Toolbar ---
    wxString resDir = GetResDir();
    auto *tbPanel = new wxPanel(mainPanel);
    tbPanel->SetBackgroundColour(wxColour(245, 245, 245));
    auto *tbSizer = new wxWrapSizer(wxHORIZONTAL);

    // Bold / Italic / Underline
    auto *btnBold = IconBtn(tbPanel, ID_BOLD, resDir, "icon_bold", "Bold (Ctrl+B)");
    auto *btnItalic = IconBtn(tbPanel, ID_ITALIC, resDir, "icon_italic", "Italic (Ctrl+I)");
    auto *btnUnderline = IconBtn(tbPanel, ID_UNDERLINE, resDir, "icon_underline", "Underline (Ctrl+U)");
    tbSizer->Add(btnBold, 0, wxALL, 1);
    tbSizer->Add(btnItalic, 0, wxALL, 1);
    tbSizer->Add(btnUnderline, 0, wxALL, 1);

    // List button (placeholder)
    auto *btnBullet = IconBtn(tbPanel, ID_BULLET, resDir, "icon_bullet", "Bullet List");
    tbSizer->Add(btnBullet, 0, wxALL, 1);

    tbSizer->Add(new wxStaticLine(tbPanel, wxID_ANY, wxDefaultPosition,
                                   wxSize(1, 24), wxLI_VERTICAL), 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);

    // Alignment
    auto *btnAlignL = IconBtn(tbPanel, ID_ALIGN_LEFT, resDir, "icon_align_left", "Align Left");
    auto *btnAlignC = IconBtn(tbPanel, ID_ALIGN_CENTER, resDir, "icon_align_center", "Align Center");
    auto *btnAlignR = IconBtn(tbPanel, ID_ALIGN_RIGHT, resDir, "icon_align_right", "Align Right");
    auto *btnJustify = IconBtn(tbPanel, ID_ALIGN_JUSTIFY, resDir, "icon_justify", "Justify");
    tbSizer->Add(btnAlignL, 0, wxALL, 1);
    tbSizer->Add(btnAlignC, 0, wxALL, 1);
    tbSizer->Add(btnAlignR, 0, wxALL, 1);
    tbSizer->Add(btnJustify, 0, wxALL, 1);

    tbSizer->Add(new wxStaticLine(tbPanel, wxID_ANY, wxDefaultPosition,
                                   wxSize(1, 24), wxLI_VERTICAL), 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);

    // Indent
    auto *btnIndentL = IconBtn(tbPanel, ID_INDENT_LEFT, resDir, "icon_indent_left", "Decrease Indent");
    auto *btnIndentR = IconBtn(tbPanel, ID_INDENT_RIGHT, resDir, "icon_indent_right", "Increase Indent");
    tbSizer->Add(btnIndentL, 0, wxALL, 1);
    tbSizer->Add(btnIndentR, 0, wxALL, 1);

    tbSizer->Add(new wxStaticLine(tbPanel, wxID_ANY, wxDefaultPosition,
                                   wxSize(1, 24), wxLI_VERTICAL), 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);

    // Undo / Redo
    auto *btnUndo = IconBtn(tbPanel, wxID_UNDO, resDir, "icon_undo", "Undo (Ctrl+Z)");
    auto *btnRedo = IconBtn(tbPanel, wxID_REDO, resDir, "icon_redo", "Redo (Ctrl+Shift+Z)");
    tbSizer->Add(btnUndo, 0, wxALL, 1);
    tbSizer->Add(btnRedo, 0, wxALL, 1);

    tbSizer->Add(new wxStaticLine(tbPanel, wxID_ANY, wxDefaultPosition,
                                   wxSize(1, 24), wxLI_VERTICAL), 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);

    // Insert: HR, Image, Link, Table
    auto *btnHR = IconBtn(tbPanel, ID_HR, resDir, "icon_hr", "Insert Horizontal Rule");
    auto *btnImage = IconBtn(tbPanel, ID_IMAGE, resDir, "icon_image", "Insert Image");
    auto *btnLink = LabelBtn(tbPanel, ID_LINK, "Link", "Insert Link (Ctrl+K)");
    auto *btnTable = LabelBtn(tbPanel, ID_TABLE, "Table", "Insert Table");
    tbSizer->Add(btnHR, 0, wxALL, 1);
    tbSizer->Add(btnImage, 0, wxALL, 1);
    tbSizer->Add(btnLink, 0, wxALL, 1);
    tbSizer->Add(btnTable, 0, wxALL, 1);

    tbSizer->Add(new wxStaticLine(tbPanel, wxID_ANY, wxDefaultPosition,
                                   wxSize(1, 24), wxLI_VERTICAL), 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);

    // Quote / Unquote / Clear / Emoji
    auto *btnQuote = LabelBtn(tbPanel, ID_QUOTE, ">", "Quote");
    auto *btnUnquote = LabelBtn(tbPanel, ID_UNQUOTE, "<", "Unquote");
    auto *btnClear = LabelBtn(tbPanel, ID_CLEAR_FMT, "Tx", "Remove Formatting (Ctrl+\\)");
    tbSizer->Add(btnQuote, 0, wxALL, 1);
    tbSizer->Add(btnUnquote, 0, wxALL, 1);
    tbSizer->Add(btnClear, 0, wxALL, 1);

    tbSizer->Add(new wxStaticLine(tbPanel, wxID_ANY, wxDefaultPosition,
                                   wxSize(1, 24), wxLI_VERTICAL), 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);

    // Font family dropdown
    wxArrayString fonts;
    fonts.Add("Default"); fonts.Add("Sans"); fonts.Add("Serif");
    fonts.Add("Monospace"); fonts.Add("Arial"); fonts.Add("Helvetica");
    fonts.Add("Times New Roman"); fonts.Add("Georgia");
    fonts.Add("Courier New"); fonts.Add("Verdana");
    fonts.Add("Trebuchet MS"); fonts.Add("Comic Sans MS");
    auto *fontChoice = new wxChoice(tbPanel, ID_FONT_FAMILY, wxDefaultPosition,
                                     wxSize(130, -1), fonts);
    fontChoice->SetSelection(0);
    fontChoice->SetToolTip("Font Family");
    tbSizer->Add(fontChoice, 0, wxALL | wxALIGN_CENTER_VERTICAL, 1);

    // Font size dropdown
    wxArrayString sizes;
    sizes.Add("Default");
    for (int s : {8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 72})
      sizes.Add(wxString::Format("%d", s));
    auto *sizeChoice = new wxChoice(tbPanel, ID_FONT_SIZE, wxDefaultPosition,
                                    wxSize(70, -1), sizes);
    sizeChoice->SetSelection(0);
    sizeChoice->SetToolTip("Font Size");
    tbSizer->Add(sizeChoice, 0, wxALL | wxALIGN_CENTER_VERTICAL, 1);

    // Foreground color picker
    auto *fgColor = new wxColourPickerCtrl(tbPanel, ID_FG_COLOR, *wxBLACK,
                                            wxDefaultPosition, wxSize(40, 28));
    fgColor->SetToolTip("Text Color");
    tbSizer->Add(fgColor, 0, wxALL | wxALIGN_CENTER_VERTICAL, 1);

    // Background color picker
    auto *bgColor = new wxColourPickerCtrl(tbPanel, ID_BG_COLOR, *wxWHITE,
                                            wxDefaultPosition, wxSize(40, 28));
    bgColor->SetToolTip("Background Color");
    tbSizer->Add(bgColor, 0, wxALL | wxALIGN_CENTER_VERTICAL, 1);

    tbSizer->AddSpacer(8);
    auto *readOnlyCheck = new wxCheckBox(tbPanel, wxID_ANY, "Read-only");
    tbSizer->Add(readOnlyCheck, 0, wxALL | wxALIGN_CENTER_VERTICAL, 2);

    tbSizer->Add(new wxStaticLine(tbPanel, wxID_ANY, wxDefaultPosition,
                                   wxSize(1, 24), wxLI_VERTICAL), 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);

    auto *btnDarkMode = LabelBtn(tbPanel, ID_DARK_MODE, "Dark", "Toggle Dark Mode");
    tbSizer->Add(btnDarkMode, 0, wxALL, 1);

    tbSizer->Add(new wxStaticLine(tbPanel, wxID_ANY, wxDefaultPosition,
                                   wxSize(1, 24), wxLI_VERTICAL), 0, wxALL | wxALIGN_CENTER_VERTICAL, 3);

    auto *btnPrint = LabelBtn(tbPanel, wxID_PRINT, "Print", "Print (Ctrl+P)");
    auto *btnPreview = LabelBtn(tbPanel, wxID_PREVIEW, "Preview", "Print Preview");
    tbSizer->Add(btnPrint, 0, wxALL, 1);
    tbSizer->Add(btnPreview, 0, wxALL, 1);

    tbPanel->SetSizer(tbSizer);
    vbox->Add(tbPanel, 0, wxEXPAND);
    vbox->Add(new wxStaticLine(mainPanel), 0, wxEXPAND);

    // --- Editor widget ---
    auto *widget = new wxHtmlEditWidget(mainPanel);
    widget->SetReadOnly(false);
    widget->GetEngine().defaultFont =
        wxFont(12, wxFONTFAMILY_DEFAULT, wxFONTSTYLE_NORMAL,
               wxFONTWEIGHT_NORMAL);
    widget->GetEngine().defaultColor = wxColour(51, 51, 51);
    widget->GetEngine().bgColor = *wxWHITE;
    widget->SetHTML(EDIT_HTML);
    vbox->Add(widget, 1, wxEXPAND);

    mainPanel->SetSizer(vbox);

    // --- Status bar ---
    frame->CreateStatusBar();
    frame->SetStatusText("Editing mode");

    // Flag to prevent feedback loop when updating controls from caret event
    auto *updating = new bool(false);

    // --- Button events ---

    // Formatting
    btnBold->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->ToggleBold(); widget->SetFocus();
    });
    btnItalic->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->ToggleItalic(); widget->SetFocus();
    });
    btnUnderline->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->ToggleUnderline(); widget->SetFocus();
    });

    // Undo / Redo
    btnUndo->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->Undo(); widget->SetFocus();
    });
    btnRedo->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->Redo(); widget->SetFocus();
    });

    // Bullet list
    btnBullet->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->ToggleBulletList(); widget->SetFocus();
    });

    // Alignment
    btnAlignL->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->SetAlignment(TextAlign::Left); widget->SetFocus();
    });
    btnAlignC->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->SetAlignment(TextAlign::Center); widget->SetFocus();
    });
    btnAlignR->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->SetAlignment(TextAlign::Right); widget->SetFocus();
    });
    btnJustify->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->SetAlignment(TextAlign::Justify); widget->SetFocus();
    });

    // Indent
    btnIndentL->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->DecreaseIndent(); widget->SetFocus();
    });
    btnIndentR->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->IncreaseIndent(); widget->SetFocus();
    });

    // Insert HR
    btnHR->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->InsertHR(); widget->SetFocus();
    });

    // Insert Image (file dialog)
    btnImage->Bind(wxEVT_BUTTON, [widget, frame](wxCommandEvent &) {
      wxFileDialog dlg(frame, "Choose Image", "", "",
                       "Image files (*.png;*.jpg;*.gif;*.bmp)|*.png;*.jpg;*.jpeg;*.gif;*.bmp",
                       wxFD_OPEN | wxFD_FILE_MUST_EXIST);
      if (dlg.ShowModal() == wxID_OK) {
        widget->InsertImage(dlg.GetPath());
        frame->SetStatusText("Inserted image: " + dlg.GetPath());
      }
      widget->SetFocus();
    });

    // Insert Link (dialog)
    btnLink->Bind(wxEVT_BUTTON, [widget, frame](wxCommandEvent &) {
      wxString sel = widget->GetSelectedText();
      wxString url = wxGetTextFromUser("Enter URL:", "Insert Link",
                                        "https://", frame);
      if (!url.IsEmpty()) {
        wxString text = sel.IsEmpty()
            ? wxGetTextFromUser("Link text:", "Insert Link", url, frame)
            : sel;
        if (!text.IsEmpty()) {
          widget->InsertLink(url, text);
        }
      }
      widget->SetFocus();
    });

    // Insert Table (dialog)
    btnTable->Bind(wxEVT_BUTTON, [widget, frame](wxCommandEvent &) {
      wxString rowsStr = wxGetTextFromUser("Rows:", "Insert Table", "3", frame);
      if (rowsStr.IsEmpty()) { widget->SetFocus(); return; }
      wxString colsStr = wxGetTextFromUser("Columns:", "Insert Table", "3", frame);
      if (colsStr.IsEmpty()) { widget->SetFocus(); return; }
      long rows = 3, cols = 3;
      rowsStr.ToLong(&rows);
      colsStr.ToLong(&cols);
      if (rows > 0 && cols > 0)
        widget->InsertTable((int)rows, (int)cols);
      widget->SetFocus();
    });

    // Quote / Unquote
    btnQuote->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->IncreaseQuoteLevel(); widget->SetFocus();
    });
    btnUnquote->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->DecreaseQuoteLevel(); widget->SetFocus();
    });

    // Clear formatting
    btnClear->Bind(wxEVT_BUTTON, [widget](wxCommandEvent &) {
      widget->RemoveFormatting(); widget->SetFocus();
    });

    // Font family
    fontChoice->Bind(wxEVT_CHOICE, [widget, fontChoice, updating](wxCommandEvent &) {
      if (*updating) return;
      static const char *families[] = {
        "", "Sans", "Serif", "Monospace",
        "Arial", "Helvetica", "Times New Roman", "Georgia",
        "Courier New", "Verdana", "Trebuchet MS", "Comic Sans MS",
      };
      int idx = fontChoice->GetSelection();
      if (idx > 0 && idx < (int)(sizeof(families) / sizeof(families[0])))
        widget->SetFontFamily(families[idx]);
      widget->SetFocus();
    });

    // Font size
    sizeChoice->Bind(wxEVT_CHOICE, [widget, sizeChoice, updating](wxCommandEvent &) {
      if (*updating) return;
      static const int szvals[] = {0, 8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 72};
      int idx = sizeChoice->GetSelection();
      if (idx > 0 && idx < (int)(sizeof(szvals) / sizeof(szvals[0])))
        widget->SetFontSize(szvals[idx]);
      widget->SetFocus();
    });

    // Colors
    fgColor->Bind(wxEVT_COLOURPICKER_CHANGED, [widget, updating](wxColourPickerEvent &evt) {
      if (*updating) return;
      widget->SetTextColor(evt.GetColour());
      widget->SetFocus();
    });
    bgColor->Bind(wxEVT_COLOURPICKER_CHANGED, [widget, updating](wxColourPickerEvent &evt) {
      if (*updating) return;
      widget->SetBackgroundColor(evt.GetColour());
      widget->SetFocus();
    });

    // Read-only toggle
    readOnlyCheck->Bind(wxEVT_CHECKBOX, [widget, frame](wxCommandEvent &evt) {
      widget->SetReadOnly(evt.IsChecked());
      frame->SetStatusText(evt.IsChecked() ? "Read-only mode" : "Editing mode");
    });

    // --- Caret tracking: update toolbar controls ---
    widget->Bind(wxEVT_HTML_CARET_CHANGED, [=](wxCommandEvent &) {
      *updating = true;
      ComputedStyle s = widget->GetCaretStyle();

      // Font family — match against dropdown entries
      static const char *families[] = {
        "", "Sans", "Serif", "Monospace",
        "Arial", "Helvetica", "Times New Roman", "Georgia",
        "Courier New", "Verdana", "Trebuchet MS", "Comic Sans MS",
      };
      wxString faceName = s.font.GetFaceName().Lower();
      int fontIdx = 0;
      for (int i = 1; i < (int)(sizeof(families) / sizeof(families[0])); ++i) {
        if (faceName == wxString(families[i]).Lower()) {
          fontIdx = i;
          break;
        }
      }
      fontChoice->SetSelection(fontIdx);

      // Font size — match closest
      static const int szvals[] = {0, 8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 72};
      int pt = s.font.GetPointSize();
      int sizeIdx = 0;
      int bestDist = 999;
      for (int i = 1; i < (int)(sizeof(szvals) / sizeof(szvals[0])); ++i) {
        int d = std::abs(szvals[i] - pt);
        if (d < bestDist) { bestDist = d; sizeIdx = i; }
      }
      sizeChoice->SetSelection(sizeIdx);

      // Colors
      if (s.color.IsOk())
        fgColor->SetColour(s.color);
      if (s.backgroundColor.IsOk())
        bgColor->SetColour(s.backgroundColor);
      else
        bgColor->SetColour(*wxWHITE);

      *updating = false;
    });

    // Link clicks
    widget->Bind(wxEVT_HTML_LINK_CLICKED, [frame](wxHtmlLinkEvent &evt) {
      frame->SetStatusText("Link: " + evt.GetURL());
    });

    // Print / Preview
    btnPrint->Bind(wxEVT_BUTTON, [widget, frame](wxCommandEvent &) {
      wxHtmlEditWidget::PrintSettings ps;
      ps.showHeader = true;
      ps.title = "wxHtmlEdit Editor";
      widget->Print(ps, frame);
    });
    btnPreview->Bind(wxEVT_BUTTON, [widget, frame](wxCommandEvent &) {
      wxHtmlEditWidget::PrintSettings ps;
      ps.showHeader = true;
      ps.title = "wxHtmlEdit Editor";
      widget->PrintPreview(ps, frame);
    });

    // Dark mode toggle
    auto *darkMode = new bool(false);
    btnDarkMode->Bind(wxEVT_BUTTON, [widget, btnDarkMode, darkMode](wxCommandEvent &) {
      *darkMode = !*darkMode;
      if (*darkMode) {
        widget->GetEngine().bgColor = wxColour(30, 30, 30);
        widget->GetEngine().defaultColor = wxColour(220, 220, 220);
        widget->SetColorScheme(wxHtmlEditWidget::ColorScheme::Dark);
        btnDarkMode->SetLabel("Light");
      } else {
        widget->GetEngine().bgColor = *wxWHITE;
        widget->GetEngine().defaultColor = wxColour(51, 51, 51);
        widget->SetColorScheme(wxHtmlEditWidget::ColorScheme::Light);
        btnDarkMode->SetLabel("Dark");
      }
      widget->SetFocus();
    });

    // Ctrl+P shortcut
    frame->Bind(wxEVT_CHAR_HOOK, [widget, frame](wxKeyEvent &evt) {
      if (evt.GetModifiers() == wxMOD_CONTROL && evt.GetKeyCode() == 'P') {
        wxHtmlEditWidget::PrintSettings ps;
        ps.showHeader = true;
        ps.title = "wxHtmlEdit Editor";
        widget->Print(ps, frame);
      } else {
        evt.Skip();
      }
    });

    frame->Show();
    widget->SetFocus();
    return true;
  }
};

wxIMPLEMENT_APP(EditDemoApp);
