# Graph Report - YouPDF  (2026-11-18)

## Corpus Check
- 246 files · ~259,585 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 5 file(s) not represented in the graph (top: (none) 2, .icns 1, .dylib 1)

## Summary
- 3826 nodes · 9139 edges · 222 communities (173 shown, 49 thin omitted)
- Extraction: 90% EXTRACTED · 10% INFERRED · 0% AMBIGUOUS · INFERRED: 937 edges (avg confidence: 0.81)
- Token cost: 56,250 input · 32,830 output

## Community Hubs (Navigation)
- YouPDF UI Constants and Themes
- PDF Encryption and Crypto
- Page Cropping and Media Boxes
- Content Stream Interpreter
- App Dialogs and Windows
- Annotations and Markup
- Icon Glyph Library
- Canvas Image and Text Tools
- Native macOS Menu Bar
- Undo and Redo History
- Menu Action Enum
- Inspector Panel
- Document Comparison
- Image Compression
- Toolbar Ui Module
- Batch Types Module
- Widgets Module
- Ai Module
- Release Module
- Organize Module
- Theme Module
- Stamps Module
- App Module
- Mod Module
- Error Module
- Matrix Module
- Measure Module
- Labels Module
- Comments Ui Module
- Home Ui Module
- Convert Module
- Edit Image Module
- Visual Parity Module
- Sign Module
- Document Module
- Redact Module
- Apply Module
- Ops Module
- Signature Ui Module
- Tesseract Module
- Theme Module
- Identity Module
- Forms Ui Module
- Forms Inherited Module
- Create Module
- Mod Module
- Links Module
- Office Export Module
- Outline Module
- Tasks Module
- Search Module
- Search Module
- Types Module
- Extract Module
- Openai Compat Module
- Marks Ui Module
- Organize Dialogs Module
- Types Module
- Write Module
- Mod Module
- Registry Module
- Text Edit Ui Module
- Comments Module
- Import Export Module
- Excel Module
- Util Module
- Watermark Module
- Attachments Ui Module
- Write Module
- Endpoint Module
- Common Module
- Message Module
- App Module
- App Module
- Burn Module
- Compare Module
- Document Module
- Headings Module
- Powerpoint Module
- Pdfium Module
- Mod Module
- Attachments Module
- Ollama Module
- Sign Ui Module
- Comments Module
- Pdfa Module
- I18n Extract Module
- Tts Module
- Outline Module
- Edit Style Module
- Forms Data Module
- Identity Module
- Types Module
- Mod Module
- Home Ui Module
- Codesets Module
- Config Module
- Comment Module
- Export Module
- Image Module
- Mod Module
- I18n Module
- Signature Module
- Surgery Module
- Organize Ui Module
- Render Module
- Word Module
- Redact Module
- Impl Pdfium Module
- Render Cache Module
- Flatten Module
- Youpdf Toolbar Keep Module
- README Module
- Home Module
- Home Module
- Mod Module
- Outline Gen Module
- PARITY AGENT SPECS Module
- Document Editor Module
- AUDIT STATUS Module
- Anthropic Module
- Ai Module
- Fonts Module
- Redact Engine Module
- Youpdf Modepill Module
- Inspector Module
- Text Helpers Module
- Visual Module
- Form Create Module
- Links Module
- Watermark Bg Module
- Youpdf View Module
- Create Module
- Util Module
- Mod Module
- Crop Module
- Smoke Module
- PARITY AGENT SPECS Module
- Document View Module
- Youpdf Modepill Selected Module
- App Module
- App Module
- Audit Fixes Module
- Audit Fixes2 Module
- Extract Module
- Page View Module
- PARITY AGENT SPECS Module
- PARITY AGENT SPECS Module
- Home Module
- App Module
- App Module
- Image Module
- Mod Module
- Page Exchange Module
- PARITY AGENT SPECS Module
- Config Module
- App Module
- Common Module
- Link Appearance Module
- PARITY AGENT SPECS Module
- Youpdf Doc Module
- App Module
- Outline Module
- Convert Text Module
- Page Resize Module
- Pdf Dates Module
- Word Count Module
- AUDIT REPORT Module
- PARITY AGENT SPECS Module
- Keyring Module
- App Module
- Mod Module
- App Module
- Export Ui Module
- Slideshow Ui Module
- GAP Module
- PLAN Module
- Home Module
- Cargo Module
- IMPROVEMENTS SUMMARY Module
- Icon Module
- YouPDF Icon Module
- STATUS Module
- AUDIT REPORT Module
- UPDF Deep Module
- AUDIT STATUS Module
- AUDIT STATUS Module
- AUDIT STATUS Module
- AUDIT STATUS Module
- AUDIT REPORT Module
- AUDIT REPORT Module
- PLAN Module
- Visual Parity Module
- Build Linux Appimage Module
- Build Macos App Module
- Build Macos Icon Module
- Build Windows Zip Module
- Install Pdfium Module
- Icon Module
- AUDIT STATUS Module
- AUDIT STATUS Module
- AUDIT REPORT Module
- AUDIT REPORT Module
- AUDIT REPORT Module
- GAP Module
- GAP Module
- GAP Module
- GAP Module
- GAP Module
- GAP Module
- GAP Module
- PLAN Module
- UPDF Deep Module
- Visual Parity Module
- Visual Parity Module
- IMPROVEMENTS SUMMARY Module
- README Module

## God Nodes (most connected - your core abstractions)
1. `Document` - 186 edges
2. `pal()` - 94 edges
3. `t()` - 86 edges
4. `YouPdfApp` - 70 edges
5. `Result` - 64 edges
6. `MenuAction` - 58 edges
7. `glyph()` - 54 edges
8. `AnnotationKind` - 47 edges
9. `Annotation` - 46 edges
10. `TextFragment` - 46 edges

## Surprising Connections (you probably didn't know these)
- `Blank renderer fallback (rendu factice)` --semantically_similar_to--> `PDFium required for rendering`  [INFERRED] [semantically similar]
  pdfium/README.md → README.md
- `ocr_language_must_be_a_language_code()` --calls--> `validate_language()`  [INFERRED]
  crates/youpdf-core/tests/audit_fixes2.rs → crates/youpdf-core/src/ocr/mod.rs
- `youpdf-core crate` --references--> `PDFium native library (Chrome engine)`  [INFERRED]
  README.md → pdfium/README.md
- `spawn_worker()` --calls--> `build_provider()`  [INFERRED]
  crates/youpdf-app/src/ai.rs → crates/youpdf-ai/src/providers/mod.rs
- `detect_page_defects()` --calls--> `get_page_labels()`  [INFERRED]
  crates/youpdf-app/src/ai.rs → crates/youpdf-core/src/organize/labels.rs

## Import Cycles
- 1-file cycle: `crates/youpdf-shared/src/error.rs -> crates/youpdf-shared/src/error.rs`
- 1-file cycle: `crates/youpdf-ai/src/providers/mod.rs -> crates/youpdf-ai/src/providers/mod.rs`
- 1-file cycle: `crates/youpdf-app/src/ai.rs -> crates/youpdf-app/src/ai.rs`
- 1-file cycle: `crates/youpdf-app/src/app/batch_types.rs -> crates/youpdf-app/src/app/batch_types.rs`
- 1-file cycle: `crates/youpdf-core/src/annotate/comment.rs -> crates/youpdf-core/src/annotate/comment.rs`
- 1-file cycle: `crates/youpdf-core/src/annotation/comments.rs -> crates/youpdf-core/src/annotation/comments.rs`
- 1-file cycle: `crates/youpdf-core/src/edit/crop.rs -> crates/youpdf-core/src/edit/crop.rs`
- 1-file cycle: `crates/youpdf-core/src/forms/mod.rs -> crates/youpdf-core/src/forms/mod.rs`
- 1-file cycle: `crates/youpdf-core/src/links.rs -> crates/youpdf-core/src/links.rs`
- 1-file cycle: `crates/youpdf-core/src/redact.rs -> crates/youpdf-core/src/redact.rs`
- 1-file cycle: `crates/youpdf-core/src/search.rs -> crates/youpdf-core/src/search.rs`
- 1-file cycle: `crates/youpdf-core/src/structure/outline.rs -> crates/youpdf-core/src/structure/outline.rs`
- 1-file cycle: `crates/youpdf-core/src/util.rs -> crates/youpdf-core/src/util.rs`
- 1-file cycle: `crates/youpdf-app/src/app/home_ui.rs -> crates/youpdf-app/src/app/home_ui.rs`
- 1-file cycle: `crates/youpdf-app/src/app/organize_ui.rs -> crates/youpdf-app/src/app/organize_ui.rs`
- 1-file cycle: `crates/youpdf-shared/src/lib.rs -> crates/youpdf-shared/src/lib.rs`
- 2-file cycle: `crates/youpdf-ai/src/provider.rs -> crates/youpdf-ai/src/registry.rs -> crates/youpdf-ai/src/provider.rs`
- 2-file cycle: `crates/youpdf-core/src/create.rs -> crates/youpdf-core/src/document.rs -> crates/youpdf-core/src/create.rs`
- 2-file cycle: `crates/youpdf-app/src/app.rs -> crates/youpdf-app/src/app/history.rs -> crates/youpdf-app/src/app.rs`
- 2-file cycle: `crates/youpdf-core/src/edit/image.rs -> crates/youpdf-core/src/edit/matrix.rs -> crates/youpdf-core/src/edit/image.rs`

## Hyperedges (group relationships)
- **Four-crate YouPDF workspace** — readme_youpdf_shared, readme_youpdf_core, readme_youpdf_ai, readme_youpdf_app [EXTRACTED 1.00]
- **UPDF feature-parity documentation set** — audit_status_parity_audit, audit_status_gap_doc, audit_status_visual_parity_doc, improvements_summary_doc [INFERRED 0.85]
- **PDFium rendering and fallback path** — pdfium_readme_pdfium_library, pdfium_readme_renderer_fallback, readme_pdfium_required [INFERRED 0.85]
- **Tier 1 — high value, low effort parity gaps** — docs_gap_presenter_view, docs_gap_open_quickly, docs_gap_read_aloud, docs_gap_document_stats, docs_gap_link_appearance, docs_gap_redaction_extras, docs_gap_measure_extras, docs_gap_security_properties_matrix [EXTRACTED 1.00]
- **Highest-value first commits (A1, A2, D1, D2, E1, B1)** — docs_audit_report_unsafe_url_scheme, docs_audit_report_ai_enabled_guard, docs_audit_report_atomic_save, docs_audit_report_delete_file_confirmation, docs_audit_report_replace_all_undo, docs_audit_report_properties_search_index [EXTRACTED 1.00]
- **Wave 6 definitive visual parity diffs** — docs_visual_parity_native_menu_bar, docs_visual_parity_toolbar_presets, docs_visual_parity_paper_style, docs_visual_parity_page_shadow, docs_visual_parity_liquid_glass, docs_visual_parity_mode_pill, docs_visual_parity_status_bar, docs_visual_parity_thumbnail_aspect, docs_visual_parity_page_history [EXTRACTED 1.00]
- **Wave 1 core-engine agents form the engine layer** — docs_parity_agent_specs_agent_a1_page_labels, docs_parity_agent_specs_agent_a2_page_resize_interleave_exchange, docs_parity_agent_specs_agent_a3_link_appearance, docs_parity_agent_specs_agent_a4_redaction_extras, docs_parity_agent_specs_agent_a5_word_count_document_stats, docs_parity_agent_specs_agent_a6_security_permissions_matrix [EXTRACTED 1.00]
- **Wave 2 UI-integration agents form the UI layer** — docs_parity_agent_specs_agent_b1_page_label_editor, docs_parity_agent_specs_agent_b2_page_resize_exchange_dialog, docs_parity_agent_specs_agent_b3_link_appearance_inspector, docs_parity_agent_specs_agent_b4_redaction_code_sets_checklist, docs_parity_agent_specs_agent_b5_properties_stats_permissions, docs_parity_agent_specs_agent_b6_push_button_action_dispatch [EXTRACTED 1.00]
- **Wave 3 new-subsystem agents form the independent subsystem batch** — docs_parity_agent_specs_agent_c1_read_aloud_tts, docs_parity_agent_specs_agent_c2_presenter_view, docs_parity_agent_specs_agent_c3_open_quickly_launcher, docs_parity_agent_specs_agent_c4_create_from_capture [EXTRACTED 1.00]
- **YouPDF App Icon Composition** — crates_youpdf_app_src_assets_icon_youpdf_app_icon, crates_youpdf_app_src_assets_icon_gradient_y_monogram, crates_youpdf_app_src_assets_icon_dark_squircle_background [EXTRACTED 1.00]
- **Document editing modes exposed in the top toolbar** — docs_screenshots_document_editor_editing_mode_toolbar, docs_screenshots_document_editor_comment_mode, docs_screenshots_document_editor_edit_mode, docs_screenshots_document_editor_form_mode, docs_screenshots_document_editor_redact_mode [EXTRACTED 1.00]
- **PDF editor main workspace layout components** — docs_screenshots_document_editor_pdf_editor_ui, docs_screenshots_document_editor_tabbed_interface, docs_screenshots_document_editor_vertical_tool_sidebar, docs_screenshots_document_editor_pages_thumbnail_panel, docs_screenshots_document_editor_editing_mode_toolbar, docs_screenshots_document_editor_page_navigation_zoom [EXTRACTED 1.00]
- **Document viewing and review workspace** — docs_screenshots_document_view_navigation_rail, docs_screenshots_document_view_pages_panel, docs_screenshots_document_view_annotation_toolbar, docs_screenshots_document_view_document_canvas, docs_screenshots_document_view_pagination_controls [EXTRACTED 1.00]
- **File Input / Entry Methods on Home Screen** — docs_screenshots_home_open_file_button, docs_screenshots_home_batch_mode, docs_screenshots_home_drag_drop_upload, docs_screenshots_home_file_search [INFERRED 0.85]
- **Home Content Navigation Tabs** — docs_screenshots_home_recents_tab, docs_screenshots_home_starred_tab, docs_screenshots_home_tools_tab [INFERRED 0.85]
- **PDF viewing and annotation workflow components** — docs_screenshots_youpdf_doc_pages_sidebar, docs_screenshots_youpdf_doc_annotation_toolbar, docs_screenshots_youpdf_doc_computational_physics_document, docs_screenshots_youpdf_doc_pdfium_engine [INFERRED 0.85]
- **Mode pills form the view switcher** — docs_screenshots_youpdf_modepill_selected_mode_pill_selector, docs_screenshots_youpdf_modepill_selected_recents_pill, docs_screenshots_youpdf_modepill_selected_starred_pill, docs_screenshots_youpdf_modepill_selected_tools_pill [EXTRACTED 1.00]
- **YouPDF landing screen core UI regions** — docs_screenshots_youpdf_modepill_selected_mode_pill_selector, docs_screenshots_youpdf_modepill_selected_recent_files_grid, docs_screenshots_youpdf_modepill_selected_sidebar_navigation, docs_screenshots_youpdf_modepill_selected_file_actions, docs_screenshots_youpdf_modepill_selected_dark_theme [EXTRACTED 1.00]
- **Mode Pill View Modes** — docs_screenshots_youpdf_modepill_mode_pill, docs_screenshots_youpdf_modepill_recents_mode, docs_screenshots_youpdf_modepill_starred_mode, docs_screenshots_youpdf_modepill_tools_mode [EXTRACTED 1.00]
- **File Opening Entry Points** — docs_screenshots_youpdf_modepill_youpdf_home_screen, docs_screenshots_youpdf_modepill_open_file_action, docs_screenshots_youpdf_modepill_batch_action, docs_screenshots_youpdf_modepill_drag_drop_file_drop, docs_screenshots_youpdf_modepill_file_search_field [INFERRED 0.85]
- **File access / import entry points on the home screen** — docs_screenshots_youpdf_toolbar_keep_global_file_search, docs_screenshots_youpdf_toolbar_keep_open_file_action, docs_screenshots_youpdf_toolbar_keep_batch_mode, docs_screenshots_youpdf_toolbar_keep_drag_drop_import [INFERRED 0.85]
- **Home screen file access flow** — docs_screenshots_youpdf_view_drag_drop_upload, docs_screenshots_youpdf_view_search_files, docs_screenshots_youpdf_view_recent_files_grid, docs_screenshots_youpdf_view_batch_processing [INFERRED 0.75]

## Communities (222 total, 49 thin omitted)

### Community 0 - "YouPDF UI Constants and Themes"
Cohesion: 0.03
Nodes (47): active_tab_follows_a_move(), ADDED_GREEN, ATTACHMENT_STAGING_MAX_AGE, BLANK_SIZES, cachet(), CANVAS_THEMES, CANVAS_TOP_GAP, CARD_H (+39 more)

### Community 1 - "PDF Encryption and Crypto"
Cohesion: 0.05
Nodes (48): aes_128_cbc_encrypt_nopad(), aes_256_cbc_encrypt_no_padding(), aes_256_cbc_encrypt_raw(), aes_cbc_iv_prefix(), build_encrypt_dict_aes(), build_encrypt_dict_r3(), compute_perms(), decrypt_error() (+40 more)

### Community 2 - "Page Cropping and Media Boxes"
Cohesion: 0.06
Nodes (20): array_box(), clamp_margins(), CropMargins, Document, PageBox, Art, Bleed, Crop (+12 more)

### Community 3 - "Content Stream Interpreter"
Cohesion: 0.06
Nodes (29): decode_text(), Interpreter, KNOWN_OPERATORS, operand_number(), union(), average_width(), BBox, .EMPTY (+21 more)

### Community 4 - "App Dialogs and Windows"
Cohesion: 0.07
Nodes (9): YouPdfApp, month_name(), YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp (+1 more)

### Community 5 - "Annotations and Markup"
Cohesion: 0.07
Nodes (10): YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, SelectedImage (+2 more)

### Community 6 - "Icon Glyph Library"
Cohesion: 0.15
Nodes (52): glyph(), icon_aspect(), icon_attach(), icon_barcode(), icon_bell(), icon_bookmark(), icon_bubble(), icon_button() (+44 more)

### Community 7 - "Canvas Image and Text Tools"
Cohesion: 0.04
Nodes (33): CalloutPending, page_image_overlay(), page_text_select(), page_tool_interaction(), CanvasFrame, ContentDrag, Move, Rotate (+25 more)

### Community 8 - "Native macOS Menu Bar"
Cohesion: 0.10
Nodes (26): add_item(), add_item_item(), add_separator(), add_standard(), build_app_menu(), build_edit_menu(), build_file_menu(), build_go_menu() (+18 more)

### Community 9 - "Undo and Redo History"
Cohesion: 0.10
Nodes (4): YouPdfApp, YouPdfApp, YouPdfApp, annots_to_page()

### Community 10 - "Menu Action Enum"
Cohesion: 0.04
Nodes (48): MenuAction, AiEnabled, Background, Batch, Bates, ClearOutline, Close, Combine (+40 more)

### Community 11 - "Inspector Panel"
Cohesion: 0.07
Nodes (32): AnnotationId, ARROW_GAP, arrow_survives_a_tiny_panel(), arrow_x(), hex_colour_is_parsed_strictly(), INSPECTOR_HEIGHT, INSPECTOR_WIDTH, InspectorPlacement (+24 more)

### Community 12 - "Document Comparison"
Cohesion: 0.04
Nodes (41): CompareView, Overlay, Report, SideBySide, EraseMode, Segment, Stroke, ExportMode (+33 more)

### Community 13 - "Image Compression"
Cohesion: 0.08
Nodes (24): compress_images(), CompressionExtras, CompressionLevel, .ALL, Ebook, Print, Screen, CompressionOptions (+16 more)

### Community 14 - "Toolbar Ui Module"
Cohesion: 0.07
Nodes (12): YouPdfApp, set_toolbar_mode(), extract_page_fragments(), draw_annots(), YouPdfApp, YouPdfApp, YouPdfApp, ZoomMode (+4 more)

### Community 15 - "Batch Types Module"
Cohesion: 0.07
Nodes (19): BatchEntry, BatchEvent, Finished, Note, Started, BatchJob, BatchStatus, Done (+11 more)

### Community 16 - "Widgets Module"
Cohesion: 0.09
Nodes (21): YouPdfApp, YouPdfApp, IconFn, MODES, YouPdfApp, render_step(), YouPdfApp, YouPdfApp (+13 more)

### Community 17 - "Ai Module"
Cohesion: 0.09
Nodes (17): AiPanel, AiReply, Chat, Error, Models, Translated, AiRequest, Chat (+9 more)

### Community 18 - "Release Module"
Cohesion: 0.05
Nodes (42): name, runs-on, steps, strategy, GITHUB_TOKEN, arch, name, os (+34 more)

### Community 19 - "Organize Module"
Cohesion: 0.10
Nodes (38): ajout_avec_un_nom_en_double_laisse_une_seule_entree(), ajout_dans_un_arbre_a_plusieurs_noeuds(), ajout_de_piece_jointe_puis_relecture(), ajout_du_meme_nom_remplace_l_entree(), descripteur_en_ligne_est_lu_extrait_et_retire(), descripteur_sans_nom_prend_la_cle_de_l_arbre(), deux_descripteurs_partageant_un_flux_gardent_leur_index(), duplicate_attachment_pdf() (+30 more)

### Community 20 - "Theme Module"
Cohesion: 0.15
Nodes (18): YouPdfApp, layout_label(), prefs_body(), INVALID_RED, VALID_GREEN, t(), atomic_write(), atomic_write_remplace_le_fichier_sans_residu() (+10 more)

### Community 21 - "Stamps Module"
Cohesion: 0.10
Nodes (18): Annotation, dynamic_stamp_text(), point_segment_distance(), smooth_polyline(), stamp_id_for_pdf_name(), stamp_name(), stamp_template(), StampTemplate (+10 more)

### Community 22 - "App Module"
Cohesion: 0.15
Nodes (35): empty_selection_is_empty(), expand_to_line_ignore_la_ligne_suivante(), expand_to_line_prend_la_ligne_entiere(), expand_to_word_eteint_le_dernier_mot(), expand_to_word_ignore_le_dernier_espace(), expand_to_word_prend_le_mot_entier(), frag(), frag_chars() (+27 more)

### Community 23 - "Mod Module"
Cohesion: 0.09
Nodes (21): EditTextDraft, estimate_text_width(), MAX_TEXT_SIZE, MIN_TEXT_SIZE, patch_font_name(), PatchFont, .ALL, Courier (+13 more)

### Community 24 - "Error Module"
Cohesion: 0.09
Nodes (13): SERVICE_NAME, geometrie_de_page_stable(), NEXT_ID, Error, Ai, Config, Internal, InvalidInput (+5 more)

### Community 25 - "Matrix Module"
Cohesion: 0.17
Nodes (7): Document, Document, Document, matrix_of(), matrix_operands(), preceding_cm(), ImageInfo

### Community 26 - "Measure Module"
Cohesion: 0.13
Nodes (28): ACCENT, FONT_NAME, LABEL_SIZE, MARKER_HALF, measure_label(), MEASURE_MARKER, measure_ops(), measure_value() (+20 more)

### Community 27 - "Labels Module"
Cohesion: 0.11
Nodes (18): get_page_label(), get_page_labels(), LabelStyle, Arabic, LetterLower, LetterUpper, RomanLower, RomanUpper (+10 more)

### Community 28 - "Comments Ui Module"
Cohesion: 0.10
Nodes (8): YouPdfApp, annot_label(), comment_state_color(), comment_state_icon(), extract_document_text(), relative_time(), author_initial(), author_initial_is_uppercased()

### Community 29 - "Home Ui Module"
Cohesion: 0.07
Nodes (3): TRAFFIC_LIGHT_INSET, sous_suite_scoree_et_triee(), subsequence_score()

### Community 30 - "Convert Module"
Cohesion: 0.08
Nodes (20): Document, escape_html(), escape_markdown(), escape_rtf(), ImageFormat, Jpeg, Png, Tiff (+12 more)

### Community 31 - "Edit Image Module"
Cohesion: 0.16
Nodes (30): load_image_rgb(), centroide(), deplacer_une_image_la_pose_ailleurs(), deux_images_sur_une_page_restent_distinctes(), document_avec_image(), miroir_horizontal_amene_la_gauche_a_droite(), miroir_vertical_amene_le_haut_en_bas(), moities_gauche_droite() (+22 more)

### Community 32 - "Visual Parity Module"
Cohesion: 0.07
Nodes (27): Subagent wave conventions, 14 UPDF feature families, Presenter view parity gap, Parity status legend (Done / Partial / Missing), Tier 3 high-effort / product-decision parity gaps, UPDF 2.5.6 parity gap analysis, UI stack — egui + wgpu + winit, UPDF AI features (GPT-5, DeepSeek R1) (+19 more)

### Community 33 - "Sign Module"
Cohesion: 0.10
Nodes (10): BYTE_RANGE_PLACEHOLDER, OID_CERT_BAG, OID_KEY_BAG, OID_MESSAGE_DIGEST, OID_SHA_256, OID_SHROUDED_KEY_BAG, OID_SIGNING_TIME, OID_X509_CERT (+2 more)

### Community 34 - "Document Module"
Cohesion: 0.11
Nodes (10): Document, next_document_id(), point_to_page(), rect_to_display(), rect_to_page(), DocumentMetadata, parse_pdf_date(), apply_order() (+2 more)

### Community 35 - "Redact Module"
Cohesion: 0.13
Nodes (9): Document, paint_black_rect(), rects_overlap(), RedactionMark, RedactionRegion, RedactionReport, NullRenderer, Renderer (+1 more)

### Community 36 - "Apply Module"
Cohesion: 0.20
Nodes (4): Document, Document, marked_dict(), marked_dict_with()

### Community 37 - "Ops Module"
Cohesion: 0.12
Nodes (21): Background, BATES_MARKER, BG_MARKER, BG_XOBJECT_NAME, Watermark, WM_FONT_NAME, WM_GS_NAME, WM_MARKER (+13 more)

### Community 38 - "Signature Ui Module"
Cohesion: 0.14
Nodes (10): encode_png(), DRAW_CANVAS, raster_font(), SIGNATURE_DPI, signature_font(), signature_font_path(), TYPED_WIDTH, unscaled_em() (+2 more)

### Community 39 - "Tesseract Module"
Cohesion: 0.12
Nodes (11): combine_languages(), invalid_language(), is_executable(), LANGUAGES, OcrLanguage, OcrWord, validate_language(), which() (+3 more)

### Community 40 - "Theme Module"
Cohesion: 0.12
Nodes (19): DARK, darken(), LIGHT, MINT, MONO, page_border(), Palette, panel_hover() (+11 more)

### Community 41 - "Identity Module"
Cohesion: 0.12
Nodes (6): certificate_matches_key(), identity_from_parts(), P12CertBag, P12SafeBag, parse_certificates(), parse_private_key()

### Community 42 - "Forms Ui Module"
Cohesion: 0.11
Nodes (20): field(), field_at_point(), field_on_page(), field_page_matches_the_canvas_page(), fields_in_rect(), FormAlign, Bottom, DistributeH (+12 more)

### Community 43 - "Forms Inherited Module"
Cohesion: 0.12
Nodes (9): merge_pages(), resize_page(), groupe_radio_a_un_widget_par_bouton(), radio_widget(), rect(), pdf_avec_texte(), pdfium_renvoie_le_texte_et_son_rectangle(), page_pleine_image_est_un_scan() (+1 more)

### Community 44 - "Create Module"
Cohesion: 0.15
Nodes (22): blank_document(), document_from_image_bytes(), document_from_images(), table_des_pages_construite_une_seule_fois(), une_mutation_reconstruit_la_table(), approx(), assert_size(), assert_sizes() (+14 more)

### Community 45 - "Mod Module"
Cohesion: 0.16
Nodes (14): appearance_states(), da_font_alias(), field_options(), field_rect(), field_value(), FieldProperties, js_action(), NewFormField (+6 more)

### Community 46 - "Links Module"
Cohesion: 0.18
Nodes (13): annot_rect(), Document, find_link_annot(), find_link_id(), get_link_appearance(), is_safe_uri(), LinkAction, GoTo (+5 more)

### Community 47 - "Office Export Module"
Cohesion: 0.22
Nodes (20): all_parts_are_well_formed_xml(), document_avec(), document_avec_contenu(), document_avec_image(), excel_detects_number_and_date_cells(), excel_export_basic(), excel_sets_column_widths(), lire_part() (+12 more)

### Community 48 - "Outline Module"
Cohesion: 0.18
Nodes (17): YouPdfApp, bookmark_all_keys(), bookmark_at(), bookmark_at_mut(), bookmark_insert_index(), bookmark_insert_index_compensates_the_removal(), bookmark_is_ancestor(), bookmark_key() (+9 more)

### Community 49 - "Tasks Module"
Cohesion: 0.09
Nodes (19): AiTask, Chat, Expand, Explain, FinancialReview, KeyPoints, LegalReview, Polish (+11 more)

### Community 50 - "Search Module"
Cohesion: 0.13
Nodes (4): DocumentStats, is_cjk(), SearchIndex, SearchResult

### Community 51 - "Search Module"
Cohesion: 0.20
Nodes (19): main(), PageBoxes, TextChar, char_boxes_for_text(), find_query(), find_query_matches_with_and_without_case(), fold_case(), is_whole_word() (+11 more)

### Community 52 - "Types Module"
Cohesion: 0.10
Nodes (13): NM_PREFIX, Document, parse_annotation(), AnnotationStyle, APPEARANCE_FONT_NAME, APPEARANCE_GS_NAME, BorderDash, Dashed (+5 more)

### Community 53 - "Extract Module"
Cohesion: 0.15
Nodes (20): Cell, cell_gap_threshold(), cells_of(), COLUMN_TOL, columns_within_tol(), Document, ExtractedTable, Frag (+12 more)

### Community 54 - "Openai Compat Module"
Cohesion: 0.12
Nodes (9): ChatRequest, ChatWireResponse, Choice, ModelEntry, ModelsResponse, OpenAiCompatProvider, WireMessage, WireResponseMessage (+1 more)

### Community 55 - "Marks Ui Module"
Cohesion: 0.12
Nodes (8): Canvas, centered_rect(), MARKS_RED, MarksUi, STORED_SIGNATURE_LIMIT, StoredSignature, YOUPDF_STAMP_FONT, YouPdfApp

### Community 56 - "Organize Dialogs Module"
Cohesion: 0.20
Nodes (3): YouPdfApp, YouPdfApp, YouPdfApp

### Community 57 - "Types Module"
Cohesion: 0.08
Nodes (19): MARKUP_KINDS, AnnotationKind, Arrow, Callout, Circle, FileAttachment, FreeText, Highlight (+11 more)

### Community 58 - "Write Module"
Cohesion: 0.21
Nodes (3): FormField, Document, Document

### Community 59 - "Mod Module"
Cohesion: 0.15
Nodes (16): OcrEngine, OcrMode, .ALL, Editable, Searchable, TextAndImage, OcrResult, make_ocr() (+8 more)

### Community 60 - "Registry Module"
Cohesion: 0.13
Nodes (4): AiProvider, AnthropicProvider, ProviderId, ProviderRegistry

### Community 61 - "Text Edit Ui Module"
Cohesion: 0.17
Nodes (3): mask_color_in(), YouPdfApp, YouPdfApp

### Community 62 - "Comments Module"
Cohesion: 0.17
Nodes (18): export_comments(), apparence_surlignage_multi_lignes(), apparence_surlignage_sans_quadrilatere(), citation_groupee_rend_un_texte_par_rectangle(), citation_multi_lignes_suit_l_ordre_de_lecture(), citation_reprend_le_texte_du_rectangle(), etat_de_revue_traverse_le_document(), export_csv_echappe_le_texte() (+10 more)

### Community 63 - "Import Export Module"
Cohesion: 0.15
Nodes (18): Document, export_csv(), export_fdf(), export_json(), export_xfdf(), FormDataFormat, json_escape(), parse_csv() (+10 more)

### Community 64 - "Excel Module"
Cohesion: 0.18
Nodes (22): CellValue, Boolean, Date, Number, Text, column_letters(), days_from_civil(), excel_cell_value() (+14 more)

### Community 65 - "Util Module"
Cohesion: 0.16
Nodes (18): decode_png_to_rgb(), Document, signature_appearance_image(), escape_sig(), find_subslice(), hex_encode(), last_startxref(), literal_string() (+10 more)

### Community 66 - "Watermark Module"
Cohesion: 0.17
Nodes (19): BandSlots, HeaderFooter, NumberStyle, Numeric, NumericTotal, Roman, to_roman(), entete_et_pied_textes_distincts() (+11 more)

### Community 67 - "Attachments Ui Module"
Cohesion: 0.16
Nodes (5): YouPdfApp, YouPdfApp, form_kind_label(), prune_attachment_staging(), safe_attachment_name()

### Community 69 - "Endpoint Module"
Cohesion: 0.17
Nodes (15): accepte_les_endpoints_legitimes(), autorise_http_pour_un_fournisseur_sans_secret(), http_client(), http_client_with_timeout(), is_loopback(), parse_base_url(), parse_base_url_inner(), parse_public_base_url() (+7 more)

### Community 70 - "Common Module"
Cohesion: 0.16
Nodes (15): body_size(), group_lines(), group_paragraphs(), is_xml_char(), office_font(), sanitize_xml(), write_zip_with_media(), excel_column_boundaries() (+7 more)

### Community 71 - "Message Module"
Cohesion: 0.19
Nodes (10): ChatOptions, ChatResponse, Message, Role, Assistant, System, User, Turn (+2 more)

### Community 72 - "App Module"
Cohesion: 0.13
Nodes (12): CachedPage, HISTORY_BYTES, history_drops_oldest_beyond_count_and_bytes(), history_entry(), HISTORY_LIMIT, trim_history(), trim_history_to(), HistoryEntry (+4 more)

### Community 73 - "App Module"
Cohesion: 0.10
Nodes (21): TOOLS_MENU, ToolsEntry, Background, Bates, Compare, Compress, Convert, Create (+13 more)

### Community 74 - "Burn Module"
Cohesion: 0.17
Nodes (17): burn_image(), clamp_px(), draw_text(), draw_text_rgb(), fill_rgb(), fill_rgba(), FONT_5X7, glyph_for() (+9 more)

### Community 75 - "Compare Module"
Cohesion: 0.17
Nodes (15): compare_lines(), compare_lines_positional(), ComparisonReport, compute_lcs(), Difference, DifferenceKind, Added, Modified (+7 more)

### Community 76 - "Document Module"
Cohesion: 0.18
Nodes (3): normalize_rotation(), PageCache, PageView

### Community 77 - "Headings Module"
Cohesion: 0.19
Nodes (10): encode_pdf_literal(), winansi_byte(), winansi_byte_pub(), DetectedHeading, Document, group_lines(), most_frequent(), OutlineStyle (+2 more)

### Community 78 - "Powerpoint Module"
Cohesion: 0.21
Nodes (19): OfficeLayout, Exact, Flow, OfficePage, powerpoint_content_types(), powerpoint_parts(), powerpoint_presentation(), powerpoint_presentation_rels() (+11 more)

### Community 79 - "Pdfium Module"
Cohesion: 0.15
Nodes (11): ancienne_conversion(), conversion_identique_octet_pour_octet(), le_parseur_pdfium_est_reutilise(), page_avec_texte(), PDFIUM, PDFIUM_LOCK, pdfium_lock(), pixels_jaunes() (+3 more)

### Community 80 - "Mod Module"
Cohesion: 0.14
Nodes (12): SignatureInfo, SignaturePlacement, SignatureRequest, SignatureStatus, Declined, Draft, Expired, Sent (+4 more)

### Community 81 - "Attachments Module"
Cohesion: 0.26
Nodes (5): AttachmentInfo, Document, object_text(), stage_attachment(), unique_stamp()

### Community 82 - "Ollama Module"
Cohesion: 0.14
Nodes (10): erreur_http_ne_divulgue_pas_le_corps(), http_error(), ChatRequest, ChatResponseWire, OllamaProvider, TagEntry, TagsResponse, WireMessage (+2 more)

### Community 83 - "Sign Ui Module"
Cohesion: 0.20
Nodes (4): LoadedCertificate, SignUi, YouPdfApp, CertificateRequest

### Community 84 - "Comments Module"
Cohesion: 0.25
Nodes (17): append_thread(), comment(), comment_date_sort_is_newest_first(), comment_filter_stacks_criteria(), comment_order(), comment_rows(), comment_rows_nest_replies_under_their_parent(), comment_rows_promote_an_orphaned_reply() (+9 more)

### Community 85 - "Pdfa Module"
Cohesion: 0.16
Nodes (12): Conformance, PdfA1b, PdfA2b, PdfE1, PdfX1a, PdfX4, Document, document_id() (+4 more)

### Community 86 - "I18n Extract Module"
Cohesion: 0.16
Nodes (7): dict_entries(), main(), scan(), strip_comments(), ensure_import(), main(), wrap()

### Community 87 - "Tts Module"
Cohesion: 0.16
Nodes (5): capture_region_interactive(), capture_window(), clipboard_as_png(), timestamp(), TtsEngine

### Community 89 - "Edit Style Module"
Cohesion: 0.23
Nodes (17): alignment_shifts_the_start_point(), carriage_returns_are_stripped(), character_spacing_is_written_and_counted(), chosen_font_is_written_and_registered(), colour_and_styles_are_written(), default_style_follows_the_fragment(), document_with_fragment(), encre() (+9 more)

### Community 90 - "Forms Data Module"
Cohesion: 0.22
Nodes (16): aller_retour_quatre_formats(), as_text(), doc_avec_champs(), drapeau_obligatoire_sans_ecraser(), drapeaux_multiligne_et_lecture_seule(), export_csv_cite(), export_fdf_conteneur(), export_json_echappe() (+8 more)

### Community 91 - "Identity Module"
Cohesion: 0.16
Nodes (4): common_name_of(), SigningIdentity, time_to_date(), dn_value()

### Community 92 - "Types Module"
Cohesion: 0.12
Nodes (11): AnnotationState, Accepted, .ALL, Cancelled, Completed, None, Rejected, CommentStateFilter (+3 more)

### Community 93 - "Mod Module"
Cohesion: 0.12
Nodes (13): BATES_POSITIONS, Bates, BatesPosition, BottomCenter, BottomLeft, BottomRight, TopCenter, TopLeft (+5 more)

### Community 94 - "Home Ui Module"
Cohesion: 0.28
Nodes (4): YouPdfApp, save_recents(), save_stars(), shorten()

### Community 95 - "Codesets Module"
Cohesion: 0.24
Nodes (13): CodeSetEntry, credit_card_detection(), date_detection(), DEFAULT_CODE_SETS, email_detection(), find_code_matches(), foia_detection(), mark_code_matches() (+5 more)

### Community 96 - "Config Module"
Cohesion: 0.19
Nodes (8): ProviderConfig, ProviderKind, Anthropic, Gemini, Ollama, OpenAi, OpenAiCompat, build_provider()

### Community 97 - "Comment Module"
Cohesion: 0.13
Nodes (13): CommentAction, Delete, Export, Jump, Reply, Send, SetState, CommentFormat (+5 more)

### Community 98 - "Export Module"
Cohesion: 0.32
Nodes (11): comment_day(), comment_pdf_date(), comment_type_name(), export_comments_csv(), export_comments_fdf(), export_comments_markdown(), export_comments_xfdf(), fdf_annot_dict() (+3 more)

### Community 99 - "Image Module"
Cohesion: 0.24
Nodes (7): crop_fractions_follow_a_quarter_turned_image(), crop_fractions_follow_an_upright_image(), crop_fractions_in_image(), response_changed(), ImageCrop, crop_bounds(), crop_pixels()

### Community 100 - "Mod Module"
Cohesion: 0.18
Nodes (6): Document, OfficeFormat, .ALL, Excel, PowerPoint, Word

### Community 101 - "I18n Module"
Cohesion: 0.18
Nodes (10): anglais_est_identite(), dictionnaire_sans_doublon_et_sans_vide(), francais_traduit_et_retombe_sur_la_cle(), Language, .ALL, language(), English, French (+2 more)

### Community 102 - "Signature Module"
Cohesion: 0.33
Nodes (11): SignOptions, validate_signatures(), identity(), options(), second_signature_is_rejected_cleanly(), signature_png(), signed_document_validates(), signed_pdf() (+3 more)

### Community 103 - "Surgery Module"
Cohesion: 0.27
Nodes (9): is_known_operator(), lookup_xobject_subtype(), overlay_operations(), set_annots(), slice_page(), SliceOutcome, TouchedImage, write_back() (+1 more)

### Community 104 - "Organize Ui Module"
Cohesion: 0.19
Nodes (12): clipboard_image(), OrganizeDialog, Extract, Insert, Replace, Split, pages_to_confirm_delete(), tout_le_document_ne_demande_rien() (+4 more)

### Community 105 - "Render Module"
Cohesion: 0.23
Nodes (5): Annotation, form_bbox(), push_text_literal(), quad_bounds(), quad_from_rect()

### Community 106 - "Word Module"
Cohesion: 0.34
Nodes (13): root_rels(), document_rels(), image_rel_id(), word_content_types(), word_document(), word_media(), word_parts_exact(), word_parts_flow() (+5 more)

### Community 107 - "Redact Module"
Cohesion: 0.24
Nodes (12): fit_block(), fit_block_from(), fit_block_from_start_matters(), fit_block_respects_floor_and_width(), line_strips(), line_strips_stack_top_down(), split_lines(), translate_blank_page_is_noop() (+4 more)

### Community 109 - "Render Cache Module"
Cohesion: 0.29
Nodes (8): PageBitmap, deux_renderers_coexistent(), le_cache_ne_melange_pas_deux_documents(), page_blanche(), pixels_jaunes(), rendre(), surlignage(), une_retouche_est_visible_au_rendu_suivant()

### Community 110 - "Flatten Module"
Cohesion: 0.33
Nodes (12): annot_dict(), annotation_pose_une_apparence(), aplatir_conserve_le_dessin(), aplatir_laisse_les_liens(), appearance_content(), enregistrer_pose_les_annotations_sans_salir_l_etat(), l_apparence_se_voit_au_rendu(), page_blanche() (+4 more)

### Community 111 - "Youpdf Toolbar Keep Module"
Cohesion: 0.19
Nodes (14): Batch mode action button, Dark theme UI with indigo/blue accents, Drag-and-drop file import ('or drop a file anywhere'), Per-file favorite (star) toggle and 'Clear' action, File card metadata display (name, size, 'Just now'), Global file search field ('Search file...'), Tagline: 'Free, local-first PDF editing', macOS window chrome (traffic lights, toolbar, avatar) (+6 more)

### Community 112 - "README Module"
Cohesion: 0.15
Nodes (11): PDFium installation (script + manual), PDFium native library (Chrome engine), Blank renderer fallback (rendu factice), Document open/edit/save data flow, PDFium required for rendering, Workspace test suite (414 tests), YouPDF project, youpdf-ai crate (+3 more)

### Community 113 - "Home Module"
Cohesion: 0.22
Nodes (10): relative_time_follows_its_buckets(), is_star(), load_recents(), load_stars(), recents_file(), relative_time(), relative_time_buckets(), stars_file() (+2 more)

### Community 114 - "Home Module"
Cohesion: 0.15
Nodes (9): HomeCategory, .ALL, Recents, Starred, Tools, HomeView, ViewMode, Grid (+1 more)

### Community 115 - "Mod Module"
Cohesion: 0.17
Nodes (11): FormFieldKind, Barcode, Button, Checkbox, Choice, Date, MultiChoice, Other (+3 more)

### Community 116 - "Outline Gen Module"
Cohesion: 0.29
Nodes (12): action_goto_traverse_une_reecriture(), body_only_document_produces_no_bookmark(), clear_outline_empties_the_tree(), destination_vers_une_page_disparue_est_abandonnee(), ecriture_du_plan_conserve_la_hierarchie(), generate_produces_expected_count_and_nesting(), generated_title_decodes_back_to_the_heading_text(), generated_top_level_order_follows_the_document() (+4 more)

### Community 117 - "PARITY AGENT SPECS Module"
Cohesion: 0.17
Nodes (5): Agent C1 — Read-Aloud / TTS, Agent C2 — Presenter View, Agent C3 — Open Quickly (⌘⇧O Launcher), Agent C4 — Create from Capture, Wave 3 — New Subsystems (4 Parallel Agents)

### Community 118 - "Document Editor Module"
Cohesion: 0.18
Nodes (13): Sample document: ACME Annual Report 2026 (revenue up 18%, EMEA renewals, enterprise tier), Comment mode (annotate / add comments), Dark theme UI design (dark chrome with purple accent on active tool), Edit mode (content editing), Top editing-mode toolbar with Tools dropdown, undo/redo, and overflow menu, Form mode (fill / edit form fields), Bottom page navigation and zoom controls (page 1 of 4, Fit width, zoom +/-), Pages panel with scrollable page thumbnails (4 pages) (+5 more)

### Community 119 - "AUDIT STATUS Module"
Cohesion: 0.18
Nodes (11): Execution Plan Waves 0–5 Landing Status, UPDF_FEATURE_INVENTORY.md §(b), docs/GAP.md — Tier 2/3 backlog, Open Backlog / Needs Attention, macOS-style Overflow Menu (Tools), YouPDF ↔ UPDF 2.5.6 Parity Audit, docs/updf-parity-visual-and-feature.md, docs/visual-parity.md — appearance diffs (+3 more)

### Community 120 - "Anthropic Module"
Cohesion: 0.24
Nodes (9): ModelInfo, ANTHROPIC_VERSION, ContentBlock, DEFAULT_BASE_URL, MessagesRequest, MessagesResponse, model(), WireMessage (+1 more)

### Community 121 - "Ai Module"
Cohesion: 0.17
Nodes (12): defect_label(), detect_page_defects(), detecte_blank_et_dimensions(), detecte_rotation_different(), PageDefect, Blank, ContainsComments, Invalid (+4 more)

### Community 122 - "Fonts Module"
Cohesion: 0.24
Nodes (8): be16(), be32(), HELVETICA, install(), load(), PINGFANG, renderable(), SF

### Community 123 - "Redact Engine Module"
Cohesion: 0.45
Nodes (11): aucune_marque_ne_touche_rien(), contenu(), deux_textes(), la_chaine_caviardee_absent_du_fichier(), le_texte_caviarde_quitte_le_flux(), le_texte_hors_zone_survit(), le_texte_superpose_est_ecrit(), marque() (+3 more)

### Community 124 - "Youpdf Modepill Module"
Cohesion: 0.20
Nodes (11): Batch Processing Action, Drag-and-Drop File Drop ("or drop a file anywhere"), File Search Field, Left Navigation Sidebar, Mode Pill Segmented Control (Recents / Starred / Tools), Open File Action, Recent Files Thumbnail Grid, Recents Mode Tab (+3 more)

### Community 125 - "Inspector Module"
Cohesion: 0.24
Nodes (11): image_controls(), ImageAction, ApplyCrop, CancelCrop, Delete, Extract, Opacity, Replace (+3 more)

### Community 126 - "Text Helpers Module"
Cohesion: 0.33
Nodes (6): current_text_len(), extract_string_literal(), new_fragment(), push_fragment(), tokenize_content(), Document

### Community 128 - "Form Create Module"
Cohesion: 0.33
Nodes (9): case_a_cocher_a_deux_etats(), champ_cree_puis_rempli(), champ_texte_cree_puis_relu(), deplacer_un_champ(), liste_garde_ses_choix(), multi_selection_drapeau(), page_blanche(), proprietes_de_champ_aller_retour() (+1 more)

### Community 129 - "Links Module"
Cohesion: 0.35
Nodes (8): action_inconnue_est_signalee(), annotation_non_lien_est_ignoree(), lien_uri_est_lu(), lien_uri_vide_est_ignore(), page_sans_lien_est_vide(), two_pages(), uri_link(), with_link()

### Community 130 - "Watermark Bg Module"
Cohesion: 0.44
Nodes (10): arriere_plan_couleur_precede_contenu(), arriere_plan_image_pose_la_ressource(), bates_step_espace_les_numeros(), bates_zero_padding_et_decalage(), clear_backgrounds_retire_seulement_les_fonds(), clear_bates_retire_seulement_bates(), content_streams(), document_avec_contenu() (+2 more)

### Community 131 - "Youpdf View Module"
Cohesion: 0.25
Nodes (9): Batch processing, Drag-and-drop file upload ("drop a file anywhere"), File thumbnail card (name, size, timestamp, star), File not found state, YouPDF Home / Recent Files Screen (UI Screenshot), Recent files grid, File search, Left sidebar navigation (+1 more)

### Community 132 - "Create Module"
Cohesion: 0.47
Nodes (6): color_space(), Document, insert_kids_after(), is_pages_node(), refresh_page_counts(), set_pages_parent()

### Community 133 - "Util Module"
Cohesion: 0.33
Nodes (9): csv_quotes_only_when_needed(), flate_compress(), flate_decompress(), flate_handles_empty_input(), flate_round_trips(), format_size(), format_size_scales_units(), page_size_label() (+1 more)

### Community 134 - "Mod Module"
Cohesion: 0.20
Nodes (10): WatermarkPosition, BottomCenter, BottomLeft, BottomRight, Center, MiddleLeft, MiddleRight, TopCenter (+2 more)

### Community 135 - "Crop Module"
Cohesion: 0.38
Nodes (9): a4(), crop_box_returns_set_box(), crop_keeps_box_origin(), crop_margins_from_inch_are_converted_to_points(), crop_margins_from_unit_are_converted_to_points(), crop_removes_margins_on_each_side(), over_large_margin_clamps_to_one_point(), pdf_with_boxes() (+1 more)

### Community 136 - "Smoke Module"
Cohesion: 0.33
Nodes (7): aes256_encrypt_layout(), annotation_roundtrip(), document_opens_and_reads_text(), pdfium_renders_real_pixels(), search_and_compare(), str_len(), tiny_pdf()

### Community 137 - "PARITY AGENT SPECS Module"
Cohesion: 0.24
Nodes (7): Agent A3 — Link Appearance, Agent A4 — Redaction Extras (Image/Page Redaction, Code Sets), Agent B3 — Link Appearance Inspector, Agent B4 — Redaction Code Sets Checklist, Agent B6 — Push-Button Action Dispatch, Wave 1 — Core Engine Gaps (6 Parallel Agents), Wave 2 — UI Integration (6 Parallel Agents, Single File Ownership)

### Community 138 - "Document View Module"
Cohesion: 0.29
Nodes (10): Top annotation toolbar (Tools, Comment, Edit, Form, Redact), Dark theme application chrome, Main document canvas showing annual report content, Document Viewer UI showing sample.pdf (ACME Annual Report 2026), Left vertical navigation/tool rail, Pages thumbnail panel, Bottom pagination and zoom/fit controls, PDFium rendering engine / Letter portrait status indicator (+2 more)

### Community 139 - "Youpdf Modepill Selected Module"
Cohesion: 0.22
Nodes (10): Dark Theme UI, File open/search/batch actions with drop-anywhere affordance, Local-first PDF editing value proposition, Mode Pill Selector (segmented view switcher), Recent Files Grid (PDF thumbnails with favorite stars), Recents Mode Pill (selected/active state, highlighted purple), Left Icon Sidebar Navigation, Starred Mode Pill (+2 more)

### Community 140 - "App Module"
Cohesion: 0.22
Nodes (9): PageAction, Blank, Delete, Duplicate, Extract, InsertFile, Move, Properties (+1 more)

### Community 141 - "App Module"
Cohesion: 0.22
Nodes (9): TabAction, Close, CloseAll, CloseOthers, CloseRight, CopyPath, Duplicate, Reveal (+1 more)

### Community 142 - "Audit Fixes Module"
Cohesion: 0.42
Nodes (8): aes256_output_is_readable_and_metadata_encrypted(), base_font_of(), doc_with_f1(), encrypted_stream_length_matches_content(), patched_text_is_winansi_not_utf8(), replace_text_paints_the_requested_colours(), signature_does_not_clobber_page_font(), signature_label_cannot_inject_operators()

### Community 143 - "Audit Fixes2 Module"
Cohesion: 0.31
Nodes (6): annots_of(), doc_with_annots(), ocr_language_must_be_a_language_code(), redaction_preserves_annotations_outside_the_redacted_area(), redaction_sur_page_tournee(), rotated_page()

### Community 144 - "Extract Module"
Cohesion: 0.47
Nodes (8): flux_avec_tableau_tj_ne_boucle_pas(), frag(), grille_trois_trois(), page_sans_texte_resultats_vides(), paires_cle_valeur_deux_colonnes(), paragraphe_sans_alignement_pas_de_tableau(), pdf_with(), tableau_trois_trois()

### Community 145 - "Page View Module"
Cohesion: 0.44
Nodes (7): aller_retour_sur_les_quatre_rotations(), assert_close(), assert_close2(), identite_sans_rotation(), recadrage_borne_la_fraction(), rotation_quatre_vingt_dix(), view()

### Community 146 - "PARITY AGENT SPECS Module"
Cohesion: 0.22
Nodes (4): Agent A2 — Page Resize, Interleaving, Page Exchange, Agent B2 — Page Resize & Exchange Dialog, Agent D2 — Whole-PDF Bilingual Translation, Wave 4 — AI Gaps (2 Parallel Agents)

### Community 147 - "PARITY AGENT SPECS Module"
Cohesion: 0.25
Nodes (7): docs/CONTRIBUTING.md, docs/GAP.md, reference notes, YouPDF — Parity Agent Specifications (Full Detail), UPDF 2.5.6, docs/visual-parity.md, Wave 0 — Collision Prep

### Community 148 - "Home Module"
Cohesion: 0.22
Nodes (9): Batch Processing Mode, Dark Theme, Drag-and-Drop File Upload ("or drop a file anywhere"), File Search Input, YouPDF Home Screen, Local-first PDF Editing Value Proposition, Open File Button, Icon Sidebar Navigation (+1 more)

### Community 149 - "App Module"
Cohesion: 0.25
Nodes (8): DraftKind, Cloud, FreeText, Link, Note, Signature, Stamp, TextDraft

### Community 150 - "App Module"
Cohesion: 0.43
Nodes (7): lien_du_dessus_lemporte(), lien_hors_de_la_boite_est_ignore(), lien_sous_le_point_est_trouve(), lien_tolere_pres_du_bord(), link(), link_at(), PageLink

### Community 151 - "Image Module"
Cohesion: 0.25
Nodes (6): ImageTransform, .ALL, FlipHorizontal, FlipVertical, RotateCcw, RotateCw

### Community 152 - "Mod Module"
Cohesion: 0.36
Nodes (3): collect_references(), MAX_OUTLINE_DEPTH, OutlineItem

### Community 153 - "Page Exchange Module"
Cohesion: 0.50
Nodes (6): exchange_disjoint_ranges(), exchange_overlapping_ranges_rejected(), interleave_even_first(), interleave_odd_first(), order(), reload()

### Community 154 - "PARITY AGENT SPECS Module"
Cohesion: 0.25
Nodes (3): Agent A1 — Page Labels, Agent B1 — Page Label Editor, Agent D1 — AI Writing Tools & AI Check Pages

### Community 156 - "App Module"
Cohesion: 0.29
Nodes (6): PageLayout, .ALL, Continuous, Cover, Single, TwoPages

### Community 157 - "Common Module"
Cohesion: 0.57
Nodes (7): hex_color(), scale_to_i64(), OfficeRun, ppt_run(), ppt_text_shape(), word_frame_paragraph(), word_run()

### Community 158 - "Link Appearance Module"
Cohesion: 0.48
Nodes (5): dashed_invisible(), doc_with_link(), roundtrip_appearance(), solid_blue_border(), unknown_link_returns_none()

### Community 159 - "PARITY AGENT SPECS Module"
Cohesion: 0.29
Nodes (3): Agent A5 — Word Count & Document Stats, Agent A6 — Security Permissions Matrix, Agent B5 — Properties Window Stats & Permissions

### Community 160 - "Youpdf Doc Module"
Cohesion: 0.29
Nodes (7): Annotation toolbar (Comment / Edit / Form / Redact), Computational physics lecture PDF (HEVS, Informatique et systèmes de communication), Dark theme UI design, Pages sidebar with page thumbnails, PDF Viewer UI (dark theme, YouPDF app), PDFium rendering engine (status bar), Tabbed document interface with open-file notification

### Community 161 - "App Module"
Cohesion: 0.33
Nodes (6): BookmarkRequest, Add, Delete, Move, Rename, SetDestination

### Community 162 - "Outline Module"
Cohesion: 0.47
Nodes (4): OutlineTarget, Action, Dest, None

### Community 163 - "Convert Text Module"
Cohesion: 0.53
Nodes (4): deux_pages(), export_html(), export_markdown(), export_texte_brut()

### Community 164 - "Page Resize Module"
Cohesion: 0.53
Nodes (5): a4_pdf(), legal_pdf(), resize_a4_to_legal(), resize_a4_to_letter(), resize_scales_from_actual_box()

### Community 165 - "Pdf Dates Module"
Cohesion: 0.60
Nodes (5): annotation_ecrit_une_date_pdf(), dates_tronquees_et_absurdes(), metadonnees_lisent_les_dates_pdf(), page_avec_info(), pdf_sans_metadonnees()

### Community 166 - "Word Count Module"
Cohesion: 0.53
Nodes (4): cjk_character_count(), english_word_split(), stats_struct(), tiny_pdf()

### Community 167 - "AUDIT REPORT Module"
Cohesion: 0.33
Nodes (5): F2 — stale STATUS / parity tracker docs, F1 — cargo test --workspace fails (pdfium unavailable), Wave verification gate (check/test/clippy/i18n), Test suite result — 414 tests, 0 failures, Authoritative verification commands

### Community 168 - "PARITY AGENT SPECS Module"
Cohesion: 0.33
Nodes (6): Element Align/Arrange Panel (deferred), LittleCMS, Office Import (DONE) — soffice shell-out, Validating PDF/A (deferred) — PDF/A-2b scope, PDF Portfolio (deferred) — /Collections wrapper, Wave 5 — Strategic Product Decisions (No Code Agents)

### Community 172 - "App Module"
Cohesion: 0.50
Nodes (4): AttachmentAction, Delete, Open, SaveAs

### Community 175 - "GAP Module"
Cohesion: 0.50
Nodes (4): UPDF reference material, Feature inventory spec, November 2026 reference note, Evidence map

### Community 176 - "PLAN Module"
Cohesion: 0.50
Nodes (3): Phased release plan (v0.1 Viewer – v0.6 Stretch), Cargo workspace crates, YouPDF — free clean-room Rust PDF editor

### Community 177 - "Home Module"
Cohesion: 0.50
Nodes (4): Empty Recent Files State, Recents Tab, Starred Tab, Tools Tab

### Community 178 - "Cargo Module"
Cohesion: 0.83
Nodes (4): youpdf-ai, youpdf-app, youpdf-core, youpdf-shared

### Community 180 - "Icon Module"
Cohesion: 0.67
Nodes (3): Dark Squircle Background, Gradient Y Monogram, YouPDF App Icon

### Community 181 - "YouPDF Icon Module"
Cohesion: 0.67
Nodes (3): YouPDF Application Icon — a dark navy rounded-square (squircle) app icon containing a centered, thick rounded gradient Y monogram; the standard brand/app-store icon for YouPDF, Gradient Squircle Icon Style — modern macOS/iOS-style rounded-square icon with near-black navy background and a vibrant cyan-to-purple gradient, giving a glossy, contemporary brand aesthetic, Y Monogram Logo Mark — stylized lowercase 'Y' rendered as one continuous smooth rounded ribbon; left arm cyan/teal, right arm magenta/purple, stem blue-purple, with a soft translucent glow where the arms cross

### Community 182 - "STATUS Module"
Cohesion: 0.67
Nodes (3): C1 — YouPdfApp ~518-line, ~283-field god struct, Phase 5 refactor — app.rs split into 20+ panel modules, Tier 1 parity status — 7 Closed, 2 Partial

### Community 184 - "UPDF Deep Module"
Cohesion: 0.67
Nodes (3): UPDF competitor comparison (Adobe, PDF Expert, Foxit), UPDF free-version limits, UPDF pricing / freemium model

## Knowledge Gaps
- **530 isolated node(s):** `OpenAi`, `OpenAiCompat`, `Anthropic`, `Gemini`, `Ollama` (+525 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 1121 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **49 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Document` connect `Document Module` to `YouPDF UI Constants and Themes`, `PDF Encryption and Crypto`, `Page Cropping and Media Boxes`, `Form Create Module`, `Create Module`, `Annotations and Markup`, `Links Module`, `Crop Module`, `Smoke Module`, `Undo and Redo History`, `Watermark Bg Module`, `Image Compression`, `Audit Fixes Module`, `Batch Types Module`, `Audit Fixes2 Module`, `Extract Module`, `Organize Module`, `Stamps Module`, `App Module`, `Mod Module`, `Error Module`, `Matrix Module`, `Measure Module`, `Labels Module`, `Mod Module`, `Page Exchange Module`, `Convert Module`, `Edit Image Module`, `Link Appearance Module`, `Sign Module`, `Redact Module`, `Apply Module`, `Ops Module`, `Convert Text Module`, `Page Resize Module`, `Pdf Dates Module`, `Word Count Module`, `Forms Inherited Module`, `Create Module`, `Mod Module`, `Links Module`, `Office Export Module`, `Outline Module`, `Search Module`, `Search Module`, `Types Module`, `Extract Module`, `Write Module`, `Mod Module`, `Comments Module`, `Import Export Module`, `Util Module`, `Watermark Module`, `Write Module`, `Common Module`, `App Module`, `Burn Module`, `Compare Module`, `Document Module`, `Headings Module`, `Pdfium Module`, `Attachments Module`, `Pdfa Module`, `Edit Style Module`, `Forms Data Module`, `Codesets Module`, `Image Module`, `Signature Module`, `Surgery Module`, `Redact Module`, `Impl Pdfium Module`, `Render Cache Module`, `Flatten Module`, `Outline Gen Module`, `Redact Engine Module`, `Text Helpers Module`?**
  _High betweenness centrality (0.230) - this node is a cross-community bridge._
- **Are the 6 inferred relationships involving `Document` (e.g. with `.from_lopdf()` and `read_permissions()`) actually correct?**
  _`Document` has 6 INFERRED edges - model-reasoned connections that need verification._
- **What connects `OpenAi`, `OpenAiCompat`, `Anthropic` to the rest of the system?**
  _530 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `YouPDF UI Constants and Themes` be split into smaller, more focused modules?**
  _Cohesion score 0.02821316614420063 - nodes in this community are weakly interconnected._
- **Why does `Result` connect `Error Module` to `PDF Encryption and Crypto`, `Page Cropping and Media Boxes`, `Create Module`, `Util Module`, `Image Compression`, `Theme Module`, `Mod Module`, `Matrix Module`, `Measure Module`, `Labels Module`, `Page Exchange Module`, `Convert Module`, `Sign Module`, `Document Module`, `Redact Module`, `Page Resize Module`, `Ops Module`, `Tesseract Module`, `Identity Module`, `Forms Inherited Module`, `Mod Module`, `Outline Module`, `Search Module`, `Extract Module`, `Openai Compat Module`, `Mod Module`, `Import Export Module`, `Util Module`, `Write Module`, `Endpoint Module`, `Common Module`, `Message Module`, `Burn Module`, `Compare Module`, `Headings Module`, `Pdfium Module`, `Mod Module`, `Attachments Module`, `Ollama Module`, `Pdfa Module`, `Tts Module`, `Config Module`, `Export Module`, `Image Module`, `Surgery Module`, `Redact Module`, `Anthropic Module`, `Text Helpers Module`?**
  _High betweenness centrality (0.056) - this node is a cross-community bridge._
- **Are the 84 inferred relationships involving `pal()` (e.g. with `.body()` and `.translate_window()`) actually correct?**
  _`pal()` has 84 INFERRED edges - model-reasoned connections that need verification._
- **Should `PDF Encryption and Crypto` be split into smaller, more focused modules?**
  _Cohesion score 0.05249569707401033 - nodes in this community are weakly interconnected._