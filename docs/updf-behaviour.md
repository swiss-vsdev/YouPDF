# UPDF behaviour — Edit mode and feature areas (read-only evidence)

Target: `/Applications/UPDF.app`, native macOS Swift/ObjC, universal Mach-O at
`Contents/MacOS/UPDF` (57 MB). English UI strings in
`Contents/Resources/en.lproj/Localizable.strings` (UTF-16 plist). Bundle paths in the binary show
`updf-macos-v2-develop`.

Clean-room rule: labels, classes and behaviour only. No UPDF asset, icon or code goes into YouPDF.

## Method

Evidence sources; every quote below came from one of these commands.

```sh
plutil -convert json /Applications/UPDF.app/Contents/Resources/en.lproj/Localizable.strings -o /tmp/updf-en.json
python3 -c "import json;d=json.load(open('/tmp/updf-en.json'));[print(repr(k),'=',repr(d[k])) for k in d if 'watermark' in k.lower()]"
strings -a /Applications/UPDF.app/Contents/MacOS/UPDF | grep -iE 'watermark|header|footer'
```

The `.strings` keys/values are the app's UI text (high confidence). The binary also embeds source
paths like `.../updf-macos-v2-develop/UPDF/Sources/Editor/Decorations/Watermark/WatermarkInspector.swift`;
those confirm class/file names, not on-screen wording. Labels inferred only from a symbol are marked
**unverified**.

## 1. Text editing in "Edit Text & Images" mode

Entry label `Edit Text & Images`; marketing variants `tools_Edit Text, Images, Links` =
`Edit Text, Images, Links`, `iap_Edit text, images, links in PDF` = `Edit text, images, links in
PDF`, plus `Quickly Edit Text in PDF` and `Edit text, image, links, watermarks, header & footer and
more.`

Editing is in place, not via a dialog (`strings -a .../UPDF | grep -aiE
'EditingBar|ElementFilter|TextElement|TextOptionsView|ElementsInspector'`): `EditingBar`,
`ElementFilterPanel`, `TextElement`, `InsertTextElement`, `ReplaceTextElement`, `TextOptionsView`,
`ElementsInspector`.

Per-element controls live in floating inspectors: `FloatingInspector`, `AttributesInspector`,
`FontAttributeView`, `FontStyleAttributeView`, `AlignmentOptionsView` (same `strings` command).
Exact control labels: `Font`, `Font Style`, `Font Size`, `Font Color`, `Increase Font Size`,
`Decrease Font Size`, `Alignment`, `Bold`, `Italic`, `Underline`, `Strikethrough`, `Superscript`,
`Subscript`, `Line Spacing`, `Paragraph Spacing After`, `Increase Indent`, `Decrease Indent`. Font
picker uses `Search font...` and `All Fonts`.

Commit/cancel is an in-place lifecycle, not a modal (same `strings` command):
`onTextEndEditing`, `textEndEditingHandler`, `controlTextDidEndEditing:`, `beginEditing`,
`endEditing`.

In-place hint (exact): `When adding text/image, press the Option key to superimpose a text box/image
on the original text.` (a Windows `Alt` variant also ships). `ElementFilterPanel` restricts pickable
elements; enum `ElementFilterOption`/`ElementFilterOptions`; labels `Text`, `Image`, `Link`, `Path`,
`Form Field` (label-to-member mapping **unverified**). Failure label `Fail to edit`.

## 2. Watermark

Tool `Watermark & Background`; sub-tools `Add Watermark`, `Create Watermark`, `Edit Watermark`,
`Remove Watermark`, `Watermark Templates`. Types and picker: `Text Watermark`,
`File Watermark (Image/PDF)`, `AI Generate Watermark` (variant `AI Generate watermark`),
`Add Tiled Watermark`.

```sh
strings -a /Applications/UPDF.app/Contents/MacOS/UPDF | grep -i watermark
# WatermarkInspector, WatermarkOptionsDialog, WatermarkLayoutPanel, WatermarkPositionView,
# WatermarkTextView, WatermarkChooseFileView, WatermarkGenerate, watermarkGenerationDialog
# textWatermark, imageWatermark, pdfWatermark, fileTiled, watermarkPolicy
```

Per-option labels (exact): `Opacity`, `Rotation Angle`, `Rotation`, `Angle`, `Layout`, `Position`,
`Page Range:`, `Ratio`, `Ratio of Watermark`, `Transparency`, `Tile Scale:`, `Tile only large
pages`, `Size`, `Color`, `Font`, `Font Size`, `Randomness of rotation angle`.

Layout and position constants (`strings -a .../UPDF | grep -xiE 'diagonal|...'`): `diagonal`,
`diagonalDown`, `diagonalUp`, `horizontal`, `vertical`, `custom`; `tiledRotated45`,
`tiledRotatedNegative45`, `fileTiled`; and the 3x3 grid `center`, `topLeft`, `topCenter`,
`topRight`, `centerLeft`, `centerRight`, `bottomLeft`, `bottomCenter`, `bottomRight`.

`WatermarkPositionView` (with `PositionButton`, `PositionMenu`) is the 3x3 grid. Page-range labels
`All Pages`, `Odd Pages Only`, `Even Pages Only` (a `Custom` option is **unverified**). Preset model:
`Click the "Presets" pop-up menu, then select "Save Current Settings as Preset".`, `Enter the Preset
Name`, `Preset Name`. Detection/removal: `There is no watermark in this file`;
`UPDF cannot find a watermark in this file. If you see a watermark, it was not added in UPDF or by
an Adobe PDFMaker, and cannot be detected as such.`; `Are you sure you want to permanently remove
the watermark?`; `Flatten Watermarks`.

## 3. Header & footer

Shipped name `Header & Footer`. Exact strings:

```
Add Header & Footer / Edit Header & Footer / Remove Header & Footer
Create Header & Footer Template / Header & Footer Templates
Left Header / Center Header / Right Header
Left Footer / Center Footer / Right Footer
Left Header Text / Center Header Text / Right Header Text
Left Footer Text / Center Footer Text / Right Footer Text
Header / Footer
```

Classes (from `strings -a .../UPDF | grep -iE 'header|footer'`): `HeaderFooterInspector`,
`HeaderFooterPreviewView`, `HeaderFooterOptionsDialog`, `HeaderFooterDateFormatView`,
`PageNumberFormatView`; constants `headerLeft`, `headerCenter`, `headerRight`, `footerLeft`,
`footerCenter`, `footerRight`.

Insertable fields (exact): `Insert Page Number`, `Insert Date`, `Insert File`, `Insert File:`,
`File Name`. Font controls reuse the Edit-mode set. Date rendering via `HeaderFooterDateFormatView`
/ `HeaderFooterDateFormat`; the user-visible date-format list is **unverified** (binary tokens
`dd/mm/yy`, `mm.dd.yy`, `d.m.yyyy`, `yyyy-mm-…` belong to the form-field `DateFormatView` under
`Sources/Editor/Inspectors/FormFieldInspector/...`). Detection:
`Un-applied Header and Footer is detected. Do you want to apply it?`;
`UPDF cannot find any headers or footers in this file. If you see header or footer information, it
was not added in UPDF or by an Adobe PDFMaker, and cannot be detected as such.`;
`Are you sure you want to permanently remove the header and footer information?`.

## 4. Page numbers

Labels: `Insert Page Number`, `Page Number`, `Page Number Format`, `Page Numbering`,
`Set page numbers`, `Customized Page Numbers`, `Start Number`. Picker `PageNumberFormatView`
(`Sources/Editor/Decorations/HeaderFooter/PageNumberFormatView.swift`). Observed format strings:

```sh
strings -a /Applications/UPDF.app/Contents/MacOS/UPDF | grep -aE '^[0-9]{1,3}[ ]?(/|of)[ ]?|^Page 1'
# 1 / n , 1 of n , Page 1 , Page 1 of n , 0 / 0
```

Formats include plain index, `1 / n`, `1 of n`, `Page 1`, `Page 1 of n`; a bare `1` format is
implied but not separately quoted, and picker order/membership is **unverified**.

Options (exact): `All Pages`, `Odd Pages Only`, `Even Pages Only`, `Page Range:`, `Set Page Range`,
`Enter Page Range`, `Apply continuous numbering only within selected page range`, `Keep page numbers,
headers, and footers`, `Enable this option to keep the header/footer text and page numbering in the
result document.` Bates numbering is separate (`Bates Numbering`, `Add Bates Numbering`,
`Bates Numbering Options`).

## 5. Compress

Labels `Compress`, `Compress PDF`, `Reduce File Size…` (menu `Reduce File Size`), `iap_PDF
Compression` = `PDF Compression`. Classes `CompressionInspector`, `CompressDialog`,
`CompressQualityButton`, `CompressButton` (`Sources/Batch/Compress/...`).

Preset wording (exact): `Choose File Quality`, `Select the Document Quality`, `Choose the output
document quality from the highest to the lowest.`, `Document Quality`, grades `High Quality`,
`Medium Quality`, `Low Quality`, `Balanced quality`; baseline `Original File (Uncompressed)`;
readout `Compressed Size:`. Mode copy: `This mode is used to create a document smaller in size than
the original but of a high enough page image and picture quality.`, `This mode is used to create a
document of a small size. Page and image resolution will be decreased to 150 dpi.`, `This mode is
used to save page images and pictures in source resolution.` Preview:
`You can select a file in the list to perform compression preview. The selected compression result
will be applied only after you click Apply.` The three grade names are **unverified**; binary has
`highDPI`/`lowDPI`, `HighCompressedImageOnlyPdf`, `compressH`/`compressL` but no confirmed label
set. OCR-coupled option `Compress images using MRC (requires OCR)`.

## 6. Protect / password

Labels `Protect`, `Protect PDF`, `Protect Using Password`, `Protect Using Password…`,
`Encrypt with Password`, `Encrypt`, `Batch Encrypt`, `Encrypt with Password...`. Classes
`EncryptionView` (`Sources/Editor/Accessories/SaveAsPanel/Encrypt/EncryptionView.swift`),
`OpenPasswordDialog`, `PermissionPasswordDialog`.

Two passwords (exact): `Document Open Password` / `Document Open Password:`, `Confirm Document Open
Password`, `Permissions Password` / `Permissions Password:`, `Confirm Permissions Password`,
`Require a password to open the document`, `Restrict editing and printing of the document`.

Permission controls (exact): `Printing:`, `Changes Allowed`, `Content Copying:`, `Content Copying
for Accessibility:`, `Document Assembly:`, `Page Extraction:`, `Commenting:`, `Fill Form`, `None`.
Binary enums confirm: `PrintingPermission`, `ChangesPermission`, `AccessPermissions`,
`FilePermissions`, `copyingForAccessibility`.

AES is explicit. `.strings` has `128-bit AES` and `256-bit AES`; binary has `aes_128`, `aes_256`,
`EncryptionView` and query `Encryption Level`. `Encryption Level` is the selector label; default
level **unverified**. Cross-field rules:
`The Document Open and Permissions Passwords cannot be the same.`; `Note: If a Document Open Password
is set, the Permissions Password will also open the document and bypass the security settings.`
Success: `File successfully protected from viewing using password.`, `File successfully protected
from editing using password.`, `File successfully protected from editing and viewing using
password.`

## 7. OCR

Labels `OCR`, `OCR Recognition`, `OCR Text Recognition`, `Recognize Text Using OCR`, `Perform OCR`,
`Scan & OCR`, `Text Recognition Settings`, `Text Recognition Settings (OCR)`, and
`The advanced OCR tool of UPDF will help you recognize text in scanned PDFs with 38 languages.`
Classes `OCRSettingsDialog` (`Sources/OCR/OCRSettingsDialog.swift`), `OCRInspector`
(`Sources/Batch/OCR/OCRInspector.swift`).

Output modes (exact): `OCR Recognition Type`, `Editable PDF (Dual-layer OCR)`, `Searchable PDF Only
(Dual-layer OCR)`, `Text and Pictures Only`, `Dual-layer OCR`. Descriptions: `This mode saves
recognized text and pictures. A PDF file of this type has a relatively small size.`, `This mode keeps
the background image and illustrations of the source document and overlays the recognized text.`,
`In this mode, the page image is retained, while the recognized text is placed into an invisible
layer underneath the image.`

Extra options (exact): `Document Language`, `Select PDF Language`, `Please choose more OCR
languages.`, `Selecting more than 3 languages will slow down recognition.`, `Enable borderless table
recognition`, `Enable PDF tagging`, `PDF/UA (requires OCR)`, `Form Field Recognition`,
`Recognition Signature`. Auto prompt `Scanned document detected. Enable OCR?`; setting
`Show OCR suggestions when a scanned document is detected`.

## 8. Compare documents

Labels (exact): `Compare`, `Compare PDF`, `Compare PDFs`, `Compare Files`, `Compare two files`,
`Select Files to Compare`, `tools_Compare Document Revisions` = `Compare Document Revisions`,
`tools_Compare PDFs` = `Compare PDFs`. Classes `ComparisonViewController`, `ComparisonPDFView`,
`ComparisonPageView`, `ComparisonFilterPanel`, `ChangesNavigator`, `ChangesPopover`,
`ChangeDetailView`, `PageCompareResult`, `ComparisonResult` (`Sources/Comparison/...`).

Legend (exact, `compare_` prefix): `compare_Old` = `Old`, `compare_New` = `New`,
`compare_Total Changes` = `Total Changes`, `compare_Insertions` = `Insertions`,
`compare_Deletions` = `Deletions`, `compare_Replacements` = `Replacements`, `compare_%1 pages` =
`%1 pages`; also `Number of changes`, `Changes`, `Apply Changes`, `Revert Changes`. Options:
`Ignore spaces when comparing`, `Ignore text and image layout`, `When comparing texts, focus only on
content changes and ignore text attributes such as font size and font color`. No-difference
`The compared documents are identical.` (variant `The documents compared are the same.`), plus
`There is no difference in the selected comparison items.` and `This is a scanned file, please
perform OCR before comparison.`

## 9. Export / convert

Labels: `Convert`, `Convert To`, `Convert to`, `Convert PDF`, `Export To`, `Export to:`, `Export to`,
`Export Format:`, `Output Format`, `PDF TO OTHER FORMATS`, `Convert and export PDFs to other file
formats.`, `Export your PDF to any format`.

Format labels with extensions (exact): `Word (.docx)` / `PDF to Word`; `Excel (.xlsx)` / `PDF to
Excel`; `PowerPoint (.pptx)` / `PDF to PowerPoint`; `Rich Text Format (.rtf)`; `Text (.txt)`;
`CSV (.csv)`; `XML (.xml)`; `HTML (.html)`; `Markdown(.md)` / `Convert to Markdown`;
`JPEG (.jpg)`; `PNG (.png)` / `PDF to Image`; `PDF to Long Image`; `Save as PDF/A`, `Save as
PDF/E…`, `Save as PDF/UA`, `Save as PDF/X…`.

Umbrella sentence: `Convert PDF to Word, Excel, CSV, PowerPoint, RTF, Text, Image, HTML, XML, and PDF/A.` Image modes: `Each page as an image`, `Combine multiple pages into a single long image`, `Page Format`, `Custom Size`, `Use custom size`. Word layout: `Export with the same layout as the source document.`, `Recover exact page presentation using text boxes in Word.`, `The converted content will be displayed in the form of text boxes, with the layout not automatically adjusting during editing, which helps to better restore and maintain the same layout as the PDF.` Scanned guard: `This is a scanned file, please perform OCR first before coverting.`

## 10. Message when a PDF has no editable text

Primary message (one exact shipped string):

```
UPDF has detected there is no editable text content in this document; your PDF document might be an image format scanned file. It is recommended to enable the OCR function to recognize text from images. To improve recognition accuracy, please correctly select the document's language type and avoid selecting languages not present in the document. OCR is an intelligent recognition technology, and its accuracy is related to the document's clarity and complexity. Each PDF document's recognition result may vary, and your understanding is appreciated.
```

Shorter variants (exact): `Document is an image format scanned file`, `Scanned document detected.
Enable OCR?`, `This document has been identified as a scanned document. We recommend enabling OCR
functionality to recognize the text content.`, `No text found`, `Your file does not contain any text
content. Please perform OCR to recognize and upload the file in order to continue using the "%@"
function.` (`%1` variant ships too). Badge `Detected %1 scanned pages` / `Detected %1 scanned page`.
Setting `Turn on the OCR when the document appears to be scanned.`

## Coverage and limits

- `.strings` quotes are exact; binary quotes come from `strings -a` on the Mach-O.
- **Unverified**: page-number format order/membership; header/footer date-format list; displayed names of the three compression grades; default AES level; enum-to-label mapping noted inline.
