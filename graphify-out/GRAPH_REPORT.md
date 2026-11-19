# Graph Report - YouPDF  (2026-11-19)

## Corpus Check
- 236 files · ~266,487 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 5 file(s) not represented in the graph (top: (none) 2, .icns 1, .dylib 1)

## Summary
- 4009 nodes · 9248 edges · 269 communities (148 shown, 121 thin omitted)
- Extraction: 90% EXTRACTED · 10% INFERRED · 0% AMBIGUOUS · INFERRED: 912 edges (avg confidence: 0.81)
- Token cost: 56,250 input · 32,830 output

## Graph Freshness
- Built from commit: `a1fc7301`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- YouPDF UI Constants and Themes
- PDF Encryption and Crypto
- blank_document
- Content Stream Interpreter
- App Dialogs and Windows
- Annotations and Markup
- Icon Glyph Library
- YouPdfApp
- Native macOS Menu Bar
- Undo and Redo History
- Menu Action Enum
- Inspector Panel
- .toolbar
- Image Compression
- Toolbar Ui Module
- Batch Types Module
- Widgets Module
- Ai Module
- Release Module
- pal
- Theme Module
- .comments_panel
- ollama.rs
- Mod Module
- Document
- Matrix Module
- Measure Module
- Labels Module
- Comments Ui Module
- watermark/ops.rs
- Convert Module
- HeaderFooter
- Visual Parity Module
- link_appearance.rs
- .toast
- ai.rs
- Apply Module
- Ops Module
- TextFormat
- ai.rs
- Theme Module
- Forms Ui Module
- outline.rs
- Create Module
- ProviderId
- Links Module
- Office Export Module
- Outline Module
- Object
- CommentFormat
- Dictionary
- watermark/mod.rs
- Openai Compat Module
- Organize Dialogs Module
- Types Module
- Write Module
- Mod Module
- Registry Module
- Text Edit Ui Module
- src/create.rs
- .attachments_panel
- Result
- Watermark Module
- Attachments Ui Module
- Write Module
- Endpoint Module
- Common Module
- Message Module
- batch_types.rs
- home.rs
- Burn Module
- Compare Module
- Powerpoint Module
- Pdfium Module
- Mod Module
- Attachments Module
- Ollama Module
- Sign Ui Module
- Comments Module
- Pdfa Module
- Tts Module
- .touch
- Edit Style Module
- Identity Module
- Types Module
- blank_document
- Codesets Module
- .home_screen
- Comment Module
- Image Module
- Mod Module
- Signature Module
- Surgery Module
- Organize Ui Module
- Render Module
- app/render.rs
- Redact Module
- Impl Pdfium Module
- Render Cache Module
- Flatten Module
- organize_ui.rs
- openai_compat.rs
- Home Module
- redact.rs
- Outline Gen Module
- PARITY AGENT SPECS Module
- Document Editor Module
- AUDIT STATUS Module
- ProviderKind
- Fonts Module
- Youpdf Modepill Module
- edit/mod.rs
- Links Module
- forms_inherited.rs
- Youpdf View Module
- Mod Module
- Smoke Module
- PARITY AGENT SPECS Module
- convert_export.rs
- App Module
- App Module
- Audit Fixes Module
- Audit Fixes2 Module
- Extract Module
- tests/crop.rs
- Home Module
- App Module
- App Module
- ImageFormat
- Mod Module
- BatesPosition
- PARITY AGENT SPECS Module
- Config Module
- App Module
- Common Module
- Link Appearance Module
- PARITY AGENT SPECS Module
- Youpdf Doc Module
- OfficeFormat
- src/util.rs
- Page Resize Module
- Pdf Dates Module
- Word Count Module
- AppSettings
- YouPdfApp
- Keyring Module
- App Module
- .stamp_window
- STATUS Module
- AUDIT REPORT Module
- AUDIT STATUS Module
- AUDIT STATUS Module
- AUDIT REPORT Module
- CONTRIBUTING Module
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
- Mod Module
- Mod Module
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
- Permissions Cluster
- README Module
- Install Pdfium Module
- Word count / document stats parity gap
- Link appearance editor parity gap
- Measure extras parity gap
- Open Quickly (⌘F) launcher parity gap
- Read-aloud / TTS parity gap
- Redaction extras parity gap
- Security properties matrix parity gap
- lopdf for structural PDF edits
- UPDF user reviews / ratings
- Liquid Glass translucency toggle
- Status bar layout mirror
- AI Suggested Questions (suggested_questions)
- Known limitations

## God Nodes (most connected - your core abstractions)
1. `Document` - 178 edges
2. `t()` - 88 edges
3. `pal()` - 81 edges
4. `YouPdfApp` - 73 edges
5. `Result` - 58 edges
6. `MenuAction` - 58 edges
7. `glyph()` - 57 edges
8. `Annotation` - 45 edges
9. `AnnotationKind` - 43 edges
10. `TextFragment` - 32 edges

## Surprising Connections (you probably didn't know these)
- `Blank renderer fallback (rendu factice)` --semantically_similar_to--> `PDFium required for rendering`  [INFERRED] [semantically similar]
  pdfium/README.md → README.md
- `TABS` --references--> `IconFn`  [INFERRED]
  crates/youpdf-app/src/app/prefs_ui.rs → crates/youpdf-app/src/app.rs
- `ocr_language_must_be_a_language_code()` --calls--> `validate_language()`  [INFERRED]
  crates/youpdf-core/tests/audit_fixes2.rs → crates/youpdf-core/src/ocr/mod.rs
- `youpdf-core crate` --references--> `PDFium native library (Chrome engine)`  [INFERRED]
  README.md → pdfium/README.md
- `code_to_action()` --references--> `MenuAction`  [EXTRACTED]
  crates/youpdf-app/src/app/menu.rs → crates/youpdf-app/src/app.rs

## Import Cycles
- 1-file cycle: `crates/youpdf-shared/src/lib.rs -> crates/youpdf-shared/src/lib.rs`
- 1-file cycle: `crates/youpdf-core/src/forms/mod.rs -> crates/youpdf-core/src/forms/mod.rs`
- 1-file cycle: `crates/youpdf-app/src/app/organize_ui.rs -> crates/youpdf-app/src/app/organize_ui.rs`
- 1-file cycle: `crates/youpdf-core/src/redact.rs -> crates/youpdf-core/src/redact.rs`
- 1-file cycle: `crates/youpdf-ai/src/providers/mod.rs -> crates/youpdf-ai/src/providers/mod.rs`
- 1-file cycle: `crates/youpdf-app/src/ai.rs -> crates/youpdf-app/src/ai.rs`
- 1-file cycle: `crates/youpdf-core/src/links.rs -> crates/youpdf-core/src/links.rs`
- 1-file cycle: `crates/youpdf-app/src/app/batch_types.rs -> crates/youpdf-app/src/app/batch_types.rs`
- 1-file cycle: `crates/youpdf-core/src/search.rs -> crates/youpdf-core/src/search.rs`
- 1-file cycle: `crates/youpdf-core/src/structure/outline.rs -> crates/youpdf-core/src/structure/outline.rs`
- 1-file cycle: `crates/youpdf-core/src/ocr/tesseract.rs -> crates/youpdf-core/src/ocr/tesseract.rs`
- 1-file cycle: `crates/youpdf-core/src/ocr/searchable.rs -> crates/youpdf-core/src/ocr/searchable.rs`
- 1-file cycle: `crates/youpdf-core/src/annotate/comment.rs -> crates/youpdf-core/src/annotate/comment.rs`
- 1-file cycle: `crates/youpdf-core/src/annotation/comments.rs -> crates/youpdf-core/src/annotation/comments.rs`
- 1-file cycle: `crates/youpdf-shared/src/error.rs -> crates/youpdf-shared/src/error.rs`
- 1-file cycle: `crates/youpdf-core/src/util.rs -> crates/youpdf-core/src/util.rs`
- 1-file cycle: `crates/youpdf-core/src/edit/crop.rs -> crates/youpdf-core/src/edit/crop.rs`
- 1-file cycle: `crates/youpdf-app/src/app/home_ui.rs -> crates/youpdf-app/src/app/home_ui.rs`
- 2-file cycle: `crates/youpdf-app/src/app.rs -> crates/youpdf-app/src/app/history.rs -> crates/youpdf-app/src/app.rs`
- 2-file cycle: `crates/youpdf-core/src/protect/mod.rs -> crates/youpdf-core/src/protect/permissions.rs -> crates/youpdf-core/src/protect/mod.rs`

## Hyperedges (group relationships)
- **Highest-value first commits (A1, A2, D1, D2, E1, B1)** — docs_audit_report_unsafe_url_scheme, docs_audit_report_ai_enabled_guard, docs_audit_report_atomic_save, docs_audit_report_delete_file_confirmation, docs_audit_report_replace_all_undo, docs_audit_report_properties_search_index [EXTRACTED 1.00]
- **Document editing modes exposed in the top toolbar** — docs_screenshots_document_editor_editing_mode_toolbar, docs_screenshots_document_editor_comment_mode, docs_screenshots_document_editor_edit_mode, docs_screenshots_document_editor_form_mode, docs_screenshots_document_editor_redact_mode [EXTRACTED 1.00]
- **Document viewing and review workspace** — docs_screenshots_document_view_navigation_rail, docs_screenshots_document_view_pages_panel, docs_screenshots_document_view_annotation_toolbar, docs_screenshots_document_view_document_canvas, docs_screenshots_document_view_pagination_controls [EXTRACTED 1.00]
- **PDF editor main workspace layout components** — docs_screenshots_document_editor_pdf_editor_ui, docs_screenshots_document_editor_tabbed_interface, docs_screenshots_document_editor_vertical_tool_sidebar, docs_screenshots_document_editor_pages_thumbnail_panel, docs_screenshots_document_editor_editing_mode_toolbar, docs_screenshots_document_editor_page_navigation_zoom [EXTRACTED 1.00]
- **Tier 1 — high value, low effort parity gaps** — docs_gap_presenter_view, docs_gap_open_quickly, docs_gap_read_aloud, docs_gap_document_stats, docs_gap_link_appearance, docs_gap_redaction_extras, docs_gap_measure_extras, docs_gap_security_properties_matrix [EXTRACTED 1.00]
- **Wave 1 core-engine agents form the engine layer** — docs_parity_agent_specs_agent_a1_page_labels, docs_parity_agent_specs_agent_a2_page_resize_interleave_exchange, docs_parity_agent_specs_agent_a3_link_appearance, docs_parity_agent_specs_agent_a4_redaction_extras, docs_parity_agent_specs_agent_a5_word_count_document_stats, docs_parity_agent_specs_agent_a6_security_permissions_matrix [EXTRACTED 1.00]
- **Wave 2 UI-integration agents form the UI layer** — docs_parity_agent_specs_agent_b1_page_label_editor, docs_parity_agent_specs_agent_b2_page_resize_exchange_dialog, docs_parity_agent_specs_agent_b3_link_appearance_inspector, docs_parity_agent_specs_agent_b4_redaction_code_sets_checklist, docs_parity_agent_specs_agent_b5_properties_stats_permissions, docs_parity_agent_specs_agent_b6_push_button_action_dispatch [EXTRACTED 1.00]
- **Wave 3 new-subsystem agents form the independent subsystem batch** — docs_parity_agent_specs_agent_c1_read_aloud_tts, docs_parity_agent_specs_agent_c2_presenter_view, docs_parity_agent_specs_agent_c3_open_quickly_launcher, docs_parity_agent_specs_agent_c4_create_from_capture [EXTRACTED 1.00]
- **Wave 6 definitive visual parity diffs** — docs_visual_parity_native_menu_bar, docs_visual_parity_toolbar_presets, docs_visual_parity_paper_style, docs_visual_parity_page_shadow, docs_visual_parity_liquid_glass, docs_visual_parity_mode_pill, docs_visual_parity_status_bar, docs_visual_parity_thumbnail_aspect, docs_visual_parity_page_history [EXTRACTED 1.00]
- **Four-crate YouPDF workspace** — readme_youpdf_shared, readme_youpdf_core, readme_youpdf_ai, readme_youpdf_app [EXTRACTED 1.00]
- **YouPDF App Icon Composition** — crates_youpdf_app_src_assets_icon_youpdf_app_icon, crates_youpdf_app_src_assets_icon_gradient_y_monogram, crates_youpdf_app_src_assets_icon_dark_squircle_background [EXTRACTED 1.00]
- **UPDF feature-parity documentation set** — audit_status_parity_audit, audit_status_gap_doc, audit_status_visual_parity_doc, improvements_summary_doc [INFERRED 0.85]
- **File Input / Entry Methods on Home Screen** — docs_screenshots_home_open_file_button, docs_screenshots_home_batch_mode, docs_screenshots_home_drag_drop_upload, docs_screenshots_home_file_search [INFERRED 0.85]
- **Home Content Navigation Tabs** — docs_screenshots_home_recents_tab, docs_screenshots_home_starred_tab, docs_screenshots_home_tools_tab [INFERRED 0.85]
- **PDFium rendering and fallback path** — pdfium_readme_pdfium_library, pdfium_readme_renderer_fallback, readme_pdfium_required [INFERRED 0.85]

## Communities (269 total, 121 thin omitted)

### Community 0 - "YouPDF UI Constants and Themes"
Cohesion: 0.02
Nodes (40): active_tab_follows_a_move(), ADDED_GREEN, ATTACHMENT_STAGING_MAX_AGE, BLANK_SIZES, cachet(), CANVAS_THEMES, CANVAS_TOP_GAP, CARD_H (+32 more)

### Community 1 - "PDF Encryption and Crypto"
Cohesion: 0.05
Nodes (48): aes_128_cbc_encrypt_nopad(), aes_256_cbc_encrypt_no_padding(), aes_256_cbc_encrypt_raw(), aes_cbc_iv_prefix(), build_encrypt_dict_aes(), build_encrypt_dict_r3(), compute_perms(), decrypt_error() (+40 more)

### Community 2 - "blank_document"
Cohesion: 0.16
Nodes (14): blank_document(), geometrie_de_page_stable(), NEXT_ID, PageBoxes, table_des_pages_construite_une_seule_fois(), une_mutation_reconstruit_la_table(), apply_conformance_pose_les_marqueurs_de_chaque_niveau(), catalog_entry() (+6 more)

### Community 3 - "Content Stream Interpreter"
Cohesion: 0.06
Nodes (76): YouPdfApp, YouPdfApp, IconFn, glyph(), icon_aspect(), icon_attach(), icon_barcode(), icon_bell() (+68 more)

### Community 4 - "App Dialogs and Windows"
Cohesion: 0.04
Nodes (47): YouPdfApp, AnnotationId, ARROW_GAP, arrow_overhang_is_symmetric_and_faces_the_anchor(), arrow_survives_a_tiny_panel(), arrow_triangle(), arrow_x(), hex_colour_is_parsed_strictly() (+39 more)

### Community 5 - "Annotations and Markup"
Cohesion: 0.06
Nodes (29): decode_text(), Interpreter, KNOWN_OPERATORS, operand_number(), union(), average_width(), BBox, .EMPTY (+21 more)

### Community 6 - "Icon Glyph Library"
Cohesion: 0.08
Nodes (10): YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp (+2 more)

### Community 7 - "YouPdfApp"
Cohesion: 0.05
Nodes (40): compare_fit_rect(), CompareView, Overlay, Report, SideBySide, ContentElement, ContentKind, Image (+32 more)

### Community 8 - "Native macOS Menu Bar"
Cohesion: 0.11
Nodes (23): add_item(), add_item_item(), add_separator(), add_standard(), build_app_menu(), build_edit_menu(), build_file_menu(), build_go_menu() (+15 more)

### Community 9 - "Undo and Redo History"
Cohesion: 0.04
Nodes (48): MenuAction, AiEnabled, Background, Batch, Bates, ClearOutline, Close, Combine (+40 more)

### Community 10 - "Menu Action Enum"
Cohesion: 0.06
Nodes (27): CalloutPending, page_tool_interaction(), CanvasFrame, FormKind, Barcode, Button, Checkbox, Choice (+19 more)

### Community 11 - "Inspector Panel"
Cohesion: 0.10
Nodes (15): Document, Document, ImageTransform, .ALL, FlipHorizontal, FlipVertical, RotateCcw, RotateCw (+7 more)

### Community 12 - ".toolbar"
Cohesion: 0.06
Nodes (11): YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, toolbar_tools(), toolbar_tools_normalizes_stale_settings() (+3 more)

### Community 13 - "Image Compression"
Cohesion: 0.08
Nodes (24): compress_images(), CompressionExtras, CompressionLevel, .ALL, Ebook, Print, Screen, CompressionOptions (+16 more)

### Community 14 - "Toolbar Ui Module"
Cohesion: 0.11
Nodes (39): page_text_select(), empty_selection_is_empty(), expand_to_line_ignore_la_ligne_suivante(), expand_to_line_prend_la_ligne_entiere(), expand_to_word_eteint_le_dernier_mot(), expand_to_word_ignore_le_dernier_espace(), expand_to_word_prend_le_mot_entier(), frag() (+31 more)

### Community 15 - "Batch Types Module"
Cohesion: 0.12
Nodes (16): appearance_states(), da_font_alias(), field_options(), field_rect(), field_value(), FieldProperties, FormField, js_action() (+8 more)

### Community 16 - "Widgets Module"
Cohesion: 0.05
Nodes (42): name, runs-on, steps, strategy, GITHUB_TOKEN, arch, name, os (+34 more)

### Community 17 - "Ai Module"
Cohesion: 0.09
Nodes (28): lien_du_dessus_lemporte(), lien_hors_de_la_boite_est_ignore(), lien_sous_le_point_est_trouve(), lien_tolere_pres_du_bord(), link(), link_at(), annot_rect(), Document (+20 more)

### Community 18 - "Release Module"
Cohesion: 0.10
Nodes (38): ajout_avec_un_nom_en_double_laisse_une_seule_entree(), ajout_dans_un_arbre_a_plusieurs_noeuds(), ajout_de_piece_jointe_puis_relecture(), ajout_du_meme_nom_remplace_l_entree(), descripteur_en_ligne_est_lu_extrait_et_retire(), descripteur_sans_nom_prend_la_cle_de_l_arbre(), deux_descripteurs_partageant_un_flux_gardent_leur_index(), duplicate_attachment_pdf() (+30 more)

### Community 19 - "pal"
Cohesion: 0.08
Nodes (17): YouPdfApp, YouPdfApp, card(), category(), layout_tile(), sidebar(), SIDEBAR_W, switch() (+9 more)

### Community 20 - "Theme Module"
Cohesion: 0.13
Nodes (3): YouPdfApp, YouPdfApp, annots_to_display()

### Community 22 - "ollama.rs"
Cohesion: 0.14
Nodes (10): erreur_http_ne_divulgue_pas_le_corps(), http_error(), ChatRequest, ChatResponseWire, OllamaProvider, TagEntry, TagsResponse, WireMessage (+2 more)

### Community 23 - "Mod Module"
Cohesion: 0.15
Nodes (7): Document, Document, Document, Watermark, marked_dict(), marked_dict_with(), pdf_watermark_ops()

### Community 24 - "Document"
Cohesion: 0.07
Nodes (20): extract_page_fragments(), Document, next_document_id(), normalize_rotation(), PageCache, point_to_page(), rect_to_display(), rect_to_page() (+12 more)

### Community 25 - "Matrix Module"
Cohesion: 0.13
Nodes (28): ACCENT, FONT_NAME, LABEL_SIZE, MARKER_HALF, measure_label(), MEASURE_MARKER, measure_ops(), measure_value() (+20 more)

### Community 26 - "Measure Module"
Cohesion: 0.11
Nodes (18): get_page_label(), get_page_labels(), LabelStyle, Arabic, LetterLower, LetterUpper, RomanLower, RomanUpper (+10 more)

### Community 27 - "Labels Module"
Cohesion: 0.13
Nodes (10): CachedPage, HISTORY_BYTES, HISTORY_LIMIT, trim_history(), trim_history_to(), HistoryEntry, page_history_back_forward_and_branch(), Tab (+2 more)

### Community 28 - "Comments Ui Module"
Cohesion: 0.16
Nodes (30): load_image_rgb(), centroide(), deplacer_une_image_la_pose_ailleurs(), deux_images_sur_une_page_restent_distinctes(), document_avec_image(), miroir_horizontal_amene_la_gauche_a_droite(), miroir_vertical_amene_le_haut_en_bas(), moities_gauche_droite() (+22 more)

### Community 29 - "watermark/ops.rs"
Cohesion: 0.35
Nodes (9): background_image_stream(), bates_ops(), header_footer_ops(), paint_watermark_text(), push_literal(), push_text_line(), push_three(), text_width() (+1 more)

### Community 30 - "Convert Module"
Cohesion: 0.07
Nodes (27): Subagent wave conventions, 14 UPDF feature families, Presenter view parity gap, Parity status legend (Done / Partial / Missing), Tier 3 high-effort / product-decision parity gaps, UPDF 2.5.6 parity gap analysis, UI stack — egui + wgpu + winit, UPDF AI features (GPT-5, DeepSeek R1) (+19 more)

### Community 31 - "HeaderFooter"
Cohesion: 0.20
Nodes (7): BandSlots, HeaderFooter, NumberStyle, Numeric, NumericTotal, Roman, to_roman()

### Community 32 - "Visual Parity Module"
Cohesion: 0.11
Nodes (10): BYTE_RANGE_PLACEHOLDER, OID_CERT_BAG, OID_KEY_BAG, OID_MESSAGE_DIGEST, OID_SHA_256, OID_SHROUDED_KEY_BAG, OID_SIGNING_TIME, OID_X509_CERT (+2 more)

### Community 33 - "link_appearance.rs"
Cohesion: 0.39
Nodes (5): dashed_invisible(), doc_with_link(), roundtrip_appearance(), solid_blue_border(), unknown_link_returns_none()

### Community 34 - ".toast"
Cohesion: 0.09
Nodes (8): YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, YouPdfApp, SelectedImage

### Community 35 - "ai.rs"
Cohesion: 0.10
Nodes (14): AiPanel, AiReply, Chat, Error, Models, Translated, AiRequest, Chat (+6 more)

### Community 37 - "Ops Module"
Cohesion: 0.05
Nodes (7): page_image_overlay(), TRAFFIC_LIGHT_INSET, ContentDrag, Move, Rotate, PresenterView, ImageId

### Community 38 - "TextFormat"
Cohesion: 0.16
Nodes (11): TextFormat, Csv, Html, Markdown, Plain, Rtf, Xml, deux_pages() (+3 more)

### Community 39 - "ai.rs"
Cohesion: 0.07
Nodes (44): MarksUi, STORED_SIGNATURE_LIMIT, StoredSignature, YOUPDF_STAMP_FONT, DRAW_CANVAS, SIGNATURE_DPI, signature_font_path(), TYPED_WIDTH (+36 more)

### Community 40 - "Theme Module"
Cohesion: 0.14
Nodes (19): body_size(), group_lines(), group_paragraphs(), hex_color(), is_xml_char(), office_font(), sanitize_xml(), scale_to_i64() (+11 more)

### Community 42 - "Forms Ui Module"
Cohesion: 0.22
Nodes (20): all_parts_are_well_formed_xml(), document_avec(), document_avec_contenu(), document_avec_image(), excel_detects_number_and_date_cells(), excel_export_basic(), excel_sets_column_widths(), lire_part() (+12 more)

### Community 43 - "outline.rs"
Cohesion: 0.06
Nodes (37): BookmarkRequest, Add, Delete, Move, Rename, SetDestination, YouPdfApp, AttachmentInfo (+29 more)

### Community 44 - "Create Module"
Cohesion: 0.13
Nodes (11): ancienne_conversion(), conversion_identique_octet_pour_octet(), le_parseur_pdfium_est_reutilise(), page_avec_texte(), PDFIUM, PDFIUM_LOCK, pdfium_lock(), pixels_jaunes() (+3 more)

### Community 45 - "ProviderId"
Cohesion: 0.09
Nodes (5): AiProvider, AnthropicProvider, OpenAiCompatProvider, ProviderId, ProviderRegistry

### Community 46 - "Links Module"
Cohesion: 0.09
Nodes (19): AiTask, Chat, Expand, Explain, FinancialReview, KeyPoints, LegalReview, Polish (+11 more)

### Community 47 - "Office Export Module"
Cohesion: 0.12
Nodes (8): DocumentStats, is_cjk(), SearchIndex, SearchResult, cjk_character_count(), english_word_split(), stats_struct(), tiny_pdf()

### Community 48 - "Outline Module"
Cohesion: 0.15
Nodes (20): Cell, cell_gap_threshold(), cells_of(), COLUMN_TOL, columns_within_tol(), Document, ExtractedTable, Frag (+12 more)

### Community 49 - "Object"
Cohesion: 0.13
Nodes (18): resize_page(), flux_avec_tableau_tj_ne_boucle_pas(), frag(), grille_trois_trois(), page_sans_texte_resultats_vides(), paires_cle_valeur_deux_colonnes(), paragraphe_sans_alignement_pas_de_tableau(), pdf_with() (+10 more)

### Community 50 - "CommentFormat"
Cohesion: 0.12
Nodes (15): CommentAction, Delete, Export, Jump, Reply, Send, SetState, author_initial() (+7 more)

### Community 51 - "Dictionary"
Cohesion: 0.05
Nodes (42): combine_languages(), invalid_language(), is_executable(), LANGUAGES, OcrEngine, OcrLanguage, OcrMode, .ALL (+34 more)

### Community 52 - "watermark/mod.rs"
Cohesion: 0.11
Nodes (15): Background, Bates, BATES_MARKER, BG_MARKER, BG_XOBJECT_NAME, PageRange, All, From (+7 more)

### Community 54 - "Openai Compat Module"
Cohesion: 0.20
Nodes (3): mask_color_in(), YouPdfApp, YouPdfApp

### Community 56 - "Organize Dialogs Module"
Cohesion: 0.17
Nodes (18): export_comments(), apparence_surlignage_multi_lignes(), apparence_surlignage_sans_quadrilatere(), citation_groupee_rend_un_texte_par_rectangle(), citation_multi_lignes_suit_l_ordre_de_lecture(), citation_reprend_le_texte_du_rectangle(), etat_de_revue_traverse_le_document(), export_csv_echappe_le_texte() (+10 more)

### Community 57 - "Types Module"
Cohesion: 0.09
Nodes (18): AnnotationKind, Arrow, Callout, Circle, FileAttachment, FreeText, Highlight, Ink (+10 more)

### Community 58 - "Write Module"
Cohesion: 0.22
Nodes (16): aller_retour_quatre_formats(), as_text(), doc_avec_champs(), drapeau_obligatoire_sans_ecraser(), drapeaux_multiligne_et_lecture_seule(), export_csv_cite(), export_fdf_conteneur(), export_json_echappe() (+8 more)

### Community 59 - "Mod Module"
Cohesion: 0.17
Nodes (23): root_rels(), CellValue, Boolean, Date, Number, Text, column_letters(), days_from_civil() (+15 more)

### Community 60 - "Registry Module"
Cohesion: 0.16
Nodes (18): decode_png_to_rgb(), Document, signature_appearance_image(), escape_sig(), find_subslice(), hex_encode(), last_startxref(), literal_string() (+10 more)

### Community 61 - "Text Edit Ui Module"
Cohesion: 0.17
Nodes (15): accepte_les_endpoints_legitimes(), autorise_http_pour_un_fournisseur_sans_secret(), http_client(), http_client_with_timeout(), is_loopback(), parse_base_url(), parse_base_url_inner(), parse_public_base_url() (+7 more)

### Community 62 - "src/create.rs"
Cohesion: 0.31
Nodes (6): color_space(), Document, insert_kids_after(), is_pages_node(), refresh_page_counts(), set_pages_parent()

### Community 63 - ".attachments_panel"
Cohesion: 0.16
Nodes (6): YouPdfApp, YouPdfApp, form_kind_label(), shorten(), prune_attachment_staging(), safe_attachment_name()

### Community 64 - "Result"
Cohesion: 0.10
Nodes (12): SERVICE_NAME, merge_pages(), Error, Ai, Config, Internal, InvalidInput, Io (+4 more)

### Community 66 - "Watermark Module"
Cohesion: 0.11
Nodes (19): ChatOptions, ChatResponse, Message, ModelInfo, Role, Assistant, System, User (+11 more)

### Community 67 - "Attachments Ui Module"
Cohesion: 0.10
Nodes (21): TOOLS_MENU, ToolsEntry, Background, Bates, Compare, Compress, Convert, Create (+13 more)

### Community 68 - "Write Module"
Cohesion: 0.08
Nodes (23): Annotation, Annotation, dynamic_stamp_text(), form_bbox(), Paintable, point_segment_distance(), push_text_literal(), quad_bounds() (+15 more)

### Community 69 - "Endpoint Module"
Cohesion: 0.13
Nodes (19): Document, export_csv(), export_fdf(), export_json(), export_xfdf(), FormDataFormat, json_escape(), parse_csv() (+11 more)

### Community 70 - "Common Module"
Cohesion: 0.20
Nodes (20): OfficeLayout, Exact, Flow, OfficePage, powerpoint_content_types(), powerpoint_parts(), powerpoint_presentation(), powerpoint_presentation_rels() (+12 more)

### Community 71 - "Message Module"
Cohesion: 0.18
Nodes (16): burn_image(), clamp_px(), draw_text(), draw_text_rgb(), fill_rgb(), fill_rgba(), FONT_5X7, glyph_for() (+8 more)

### Community 72 - "batch_types.rs"
Cohesion: 0.07
Nodes (17): BatchEntry, BatchEvent, Finished, Note, Started, BatchJob, BatchStatus, Done (+9 more)

### Community 73 - "home.rs"
Cohesion: 0.10
Nodes (20): HomeCategory, Recents, Starred, Tools, HomeView, is_star(), load_opened_times(), load_recents() (+12 more)

### Community 74 - "Burn Module"
Cohesion: 0.17
Nodes (15): compare_lines(), compare_lines_positional(), ComparisonReport, compute_lcs(), Difference, DifferenceKind, Added, Modified (+7 more)

### Community 75 - "Compare Module"
Cohesion: 0.14
Nodes (12): SignatureInfo, SignaturePlacement, SignatureRequest, SignatureStatus, Declined, Draft, Expired, Sent (+4 more)

### Community 78 - "Powerpoint Module"
Cohesion: 0.16
Nodes (11): NullRenderer, PageBitmap, RenderOptions, PdfiumRenderer, deux_renderers_coexistent(), le_cache_ne_melange_pas_deux_documents(), page_blanche(), pixels_jaunes() (+3 more)

### Community 79 - "Pdfium Module"
Cohesion: 0.10
Nodes (14): NM_PREFIX, Document, parse_annotation(), AnnotationStyle, APPEARANCE_FONT_NAME, APPEARANCE_GS_NAME, BorderDash, Dashed (+6 more)

### Community 81 - "Attachments Module"
Cohesion: 0.10
Nodes (28): AnnotationState, Accepted, .ALL, Cancelled, Completed, None, Rejected, append_thread() (+20 more)

### Community 82 - "Ollama Module"
Cohesion: 0.30
Nodes (15): TextChar, char_boxes_for_text(), find_query(), find_query_matches_with_and_without_case(), is_whole_word(), match_rects(), match_rects_best(), match_rects_places_only_accepted_matches() (+7 more)

### Community 83 - "Sign Ui Module"
Cohesion: 0.16
Nodes (12): Conformance, PdfA1b, PdfA2b, PdfE1, PdfX1a, PdfX4, Document, document_id() (+4 more)

### Community 84 - "Comments Module"
Cohesion: 0.16
Nodes (7): dict_entries(), main(), scan(), strip_comments(), ensure_import(), main(), wrap()

### Community 85 - "Pdfa Module"
Cohesion: 0.16
Nodes (5): capture_region_interactive(), capture_window(), clipboard_as_png(), timestamp(), TtsEngine

### Community 87 - "Tts Module"
Cohesion: 0.32
Nodes (11): comment_day(), comment_pdf_date(), comment_type_name(), export_comments_csv(), export_comments_fdf(), export_comments_markdown(), export_comments_xfdf(), fdf_annot_dict() (+3 more)

### Community 88 - ".touch"
Cohesion: 0.06
Nodes (20): array_box(), clamp_margins(), CropMargins, Document, PageBox, Art, Bleed, Crop (+12 more)

### Community 89 - "Edit Style Module"
Cohesion: 0.23
Nodes (17): alignment_shifts_the_start_point(), carriage_returns_are_stripped(), character_spacing_is_written_and_counted(), chosen_font_is_written_and_registered(), colour_and_styles_are_written(), default_style_follows_the_fragment(), document_with_fragment(), encre() (+9 more)

### Community 91 - "Identity Module"
Cohesion: 0.10
Nodes (21): FormFieldKind, Barcode, Button, Checkbox, Choice, Date, MultiChoice, Other (+13 more)

### Community 92 - "Types Module"
Cohesion: 0.31
Nodes (14): Part, document_rels(), image_rel_id(), word_content_types(), word_document(), word_flow_body(), word_media(), word_parts_exact() (+6 more)

### Community 94 - "blank_document"
Cohesion: 0.33
Nodes (11): document_from_image_bytes(), document_from_images(), approx(), assert_size(), assert_sizes(), blank_document_a_bon_taille_et_se_reouvre(), document_from_image_bytes_donne_le_meme_document(), document_from_images_une_page_par_image() (+3 more)

### Community 95 - "Codesets Module"
Cohesion: 0.08
Nodes (19): field(), field_at_point(), field_on_page(), field_page_matches_the_canvas_page(), fields_in_rect(), FormAlign, Bottom, DistributeH (+11 more)

### Community 96 - ".home_screen"
Cohesion: 0.15
Nodes (11): accent_button(), bold_text(), fitted_thumb(), grid_layout(), paint_icon(), paint_tilted(), SIDE_GAP, sous_suite_scoree_et_triee() (+3 more)

### Community 97 - "Comment Module"
Cohesion: 0.24
Nodes (13): CodeSetEntry, credit_card_detection(), date_detection(), DEFAULT_CODE_SETS, email_detection(), find_code_matches(), foia_detection(), mark_code_matches() (+5 more)

### Community 99 - "Image Module"
Cohesion: 0.25
Nodes (7): DetectedHeading, Document, group_lines(), most_frequent(), OutlineStyle, size_key(), TextLine

### Community 100 - "Mod Module"
Cohesion: 0.15
Nodes (7): certificate_matches_key(), common_name_of(), identity_from_parts(), parse_certificates(), parse_private_key(), SigningIdentity, time_to_date()

### Community 102 - "Signature Module"
Cohesion: 0.18
Nodes (10): signature_font(), be16(), be32(), HELVETICA, install(), load(), PINGFANG, renderable() (+2 more)

### Community 103 - "Surgery Module"
Cohesion: 0.18
Nodes (10): anglais_est_identite(), dictionnaire_sans_doublon_et_sans_vide(), francais_traduit_et_retombe_sur_la_cle(), Language, .ALL, language(), English, French (+2 more)

### Community 104 - "Organize Ui Module"
Cohesion: 0.30
Nodes (12): COVER_STATE, SignOptions, validate_signatures(), identity(), options(), second_signature_is_rejected_cleanly(), signature_png(), signed_document_validates() (+4 more)

### Community 105 - "Render Module"
Cohesion: 0.21
Nodes (6): FluentIcons, Icon, Folder, Gear, Magic, load()

### Community 106 - "app/render.rs"
Cohesion: 0.12
Nodes (8): Theme, annot_label(), apply_theme(), comment_state_color(), comment_state_icon(), draw_annots(), extract_document_text(), relative_time()

### Community 109 - "Render Cache Module"
Cohesion: 0.33
Nodes (12): annot_dict(), annotation_pose_une_apparence(), aplatir_conserve_le_dessin(), aplatir_laisse_les_liens(), appearance_content(), enregistrer_pose_les_annotations_sans_salir_l_etat(), l_apparence_se_voit_au_rendu(), page_blanche() (+4 more)

### Community 110 - "Flatten Module"
Cohesion: 0.15
Nodes (11): PDFium installation (script + manual), PDFium native library (Chrome engine), Blank renderer fallback (rendu factice), Document open/edit/save data flow, PDFium required for rendering, Workspace test suite (414 tests), YouPDF project, youpdf-ai crate (+3 more)

### Community 112 - "organize_ui.rs"
Cohesion: 0.23
Nodes (10): OrganizeDialog, Extract, Insert, Replace, Split, pages_to_confirm_delete(), tout_le_document_ne_demande_rien(), tout_sauf_une_page_demande_confirmation() (+2 more)

### Community 113 - "openai_compat.rs"
Cohesion: 0.27
Nodes (8): ChatRequest, ChatWireResponse, Choice, ModelEntry, ModelsResponse, WireMessage, WireResponseMessage, WireUsage

### Community 115 - "redact.rs"
Cohesion: 0.13
Nodes (18): Document, fit_block(), fit_block_from(), fit_block_from_start_matters(), fit_block_respects_floor_and_width(), line_strips(), line_strips_stack_top_down(), paint_black_rect() (+10 more)

### Community 116 - "Outline Gen Module"
Cohesion: 0.29
Nodes (12): action_goto_traverse_une_reecriture(), body_only_document_produces_no_bookmark(), clear_outline_empties_the_tree(), destination_vers_une_page_disparue_est_abandonnee(), ecriture_du_plan_conserve_la_hierarchie(), generate_produces_expected_count_and_nesting(), generated_title_decodes_back_to_the_heading_text(), generated_top_level_order_follows_the_document() (+4 more)

### Community 117 - "PARITY AGENT SPECS Module"
Cohesion: 0.41
Nodes (12): entete_et_pied_textes_distincts(), entete_pied_seulement_pages_choisies(), filigrane_present_apres_enregistrement(), macros_entete_remplacees(), numero_ignore_premiere_page(), numeros_de_page_formats(), page_content(), pages_blanches() (+4 more)

### Community 118 - "Document Editor Module"
Cohesion: 0.17
Nodes (5): Agent C1 — Read-Aloud / TTS, Agent C2 — Presenter View, Agent C3 — Open Quickly (⌘⇧O Launcher), Agent C4 — Create from Capture, Wave 3 — New Subsystems (4 Parallel Agents)

### Community 119 - "AUDIT STATUS Module"
Cohesion: 0.18
Nodes (13): Sample document: ACME Annual Report 2026 (revenue up 18%, EMEA renewals, enterprise tier), Comment mode (annotate / add comments), Dark theme UI design (dark chrome with purple accent on active tool), Edit mode (content editing), Top editing-mode toolbar with Tools dropdown, undo/redo, and overflow menu, Form mode (fill / edit form fields), Bottom page navigation and zoom controls (page 1 of 4, Fit width, zoom +/-), Pages panel with scrollable page thumbnails (4 pages) (+5 more)

### Community 120 - "ProviderKind"
Cohesion: 0.16
Nodes (10): ProviderConfig, ProviderKind, Anthropic, Gemini, Ollama, OpenAi, OpenAiCompat, SecretStore (+2 more)

### Community 122 - "Fonts Module"
Cohesion: 0.18
Nodes (11): Execution Plan Waves 0–5 Landing Status, UPDF_FEATURE_INVENTORY.md §(b), docs/GAP.md — Tier 2/3 backlog, Open Backlog / Needs Attention, macOS-style Overflow Menu (Tools), YouPDF ↔ UPDF 2.5.6 Parity Audit, docs/updf-parity-visual-and-feature.md, docs/visual-parity.md — appearance diffs (+3 more)

### Community 124 - "Youpdf Modepill Module"
Cohesion: 0.29
Nodes (6): PageLayout, .ALL, Continuous, Cover, Single, TwoPages

### Community 127 - "edit/mod.rs"
Cohesion: 0.09
Nodes (24): EditTextDraft, crop_fractions_follow_a_quarter_turned_image(), crop_fractions_follow_an_upright_image(), crop_fractions_in_image(), response_changed(), ImageCrop, estimate_text_width(), MAX_TEXT_SIZE (+16 more)

### Community 129 - "Links Module"
Cohesion: 0.21
Nodes (17): PatchFont, .ALL, Courier, Helvetica, Times, RedactionOverlay, aucune_marque_ne_touche_rien(), contenu() (+9 more)

### Community 130 - "forms_inherited.rs"
Cohesion: 0.31
Nodes (3): groupe_radio_a_un_widget_par_bouton(), radio_widget(), rect()

### Community 131 - "Youpdf View Module"
Cohesion: 0.18
Nodes (10): Panel, Aspect, Attachments, Bookmarks, Comments, Forms, Search, Thumbnails (+2 more)

### Community 136 - "Smoke Module"
Cohesion: 0.44
Nodes (10): arriere_plan_couleur_precede_contenu(), arriere_plan_image_pose_la_ressource(), bates_step_espace_les_numeros(), bates_zero_padding_et_decalage(), clear_backgrounds_retire_seulement_les_fonds(), clear_bates_retire_seulement_bates(), content_streams(), document_avec_contenu() (+2 more)

### Community 137 - "PARITY AGENT SPECS Module"
Cohesion: 0.20
Nodes (10): draft_kind_for(), DraftKind, Cloud, FreeText, Link, Note, Signature, Stamp (+2 more)

### Community 138 - "convert_export.rs"
Cohesion: 0.53
Nodes (4): csv_guillemets(), document_avec(), rtf_echappement(), xml_echappement()

### Community 140 - "App Module"
Cohesion: 0.20
Nodes (10): WatermarkPosition, BottomCenter, BottomLeft, BottomRight, Center, MiddleLeft, MiddleRight, TopCenter (+2 more)

### Community 141 - "App Module"
Cohesion: 0.21
Nodes (7): aes256_encrypt_layout(), annotation_roundtrip(), document_opens_and_reads_text(), pdfium_renders_real_pixels(), search_and_compare(), str_len(), tiny_pdf()

### Community 142 - "Audit Fixes Module"
Cohesion: 0.24
Nodes (6): Theme, .ALL, Dark, Light, Mint, Starry

### Community 143 - "Audit Fixes2 Module"
Cohesion: 0.24
Nodes (7): Agent A3 — Link Appearance, Agent A4 — Redaction Extras (Image/Page Redaction, Code Sets), Agent B3 — Link Appearance Inspector, Agent B4 — Redaction Code Sets Checklist, Agent B6 — Push-Button Action Dispatch, Wave 1 — Core Engine Gaps (6 Parallel Agents), Wave 2 — UI Integration (6 Parallel Agents, Single File Ownership)

### Community 144 - "Extract Module"
Cohesion: 0.29
Nodes (10): Top annotation toolbar (Tools, Comment, Edit, Form, Redact), Dark theme application chrome, Main document canvas showing annual report content, Document Viewer UI showing sample.pdf (ACME Annual Report 2026), Left vertical navigation/tool rail, Pages thumbnail panel, Bottom pagination and zoom/fit controls, PDFium rendering engine / Letter portrait status indicator (+2 more)

### Community 147 - "tests/crop.rs"
Cohesion: 0.38
Nodes (9): a4(), crop_box_returns_set_box(), crop_keeps_box_origin(), crop_margins_from_inch_are_converted_to_points(), crop_margins_from_unit_are_converted_to_points(), crop_removes_margins_on_each_side(), over_large_margin_clamps_to_one_point(), pdf_with_boxes() (+1 more)

### Community 149 - "App Module"
Cohesion: 0.22
Nodes (9): PageAction, Blank, Delete, Duplicate, Extract, InsertFile, Move, Properties (+1 more)

### Community 150 - "App Module"
Cohesion: 0.22
Nodes (9): TabAction, Close, CloseAll, CloseOthers, CloseRight, CopyPath, Duplicate, Reveal (+1 more)

### Community 151 - "ImageFormat"
Cohesion: 0.33
Nodes (4): ImageFormat, Jpeg, Png, Tiff

### Community 152 - "Mod Module"
Cohesion: 0.47
Nodes (6): TextFragment, current_text_len(), extract_string_literal(), new_fragment(), push_fragment(), tokenize_content()

### Community 153 - "BatesPosition"
Cohesion: 0.25
Nodes (8): BATES_POSITIONS, BatesPosition, BottomCenter, BottomLeft, BottomRight, TopCenter, TopLeft, TopRight

### Community 154 - "PARITY AGENT SPECS Module"
Cohesion: 0.42
Nodes (8): aes256_output_is_readable_and_metadata_encrypted(), base_font_of(), doc_with_f1(), encrypted_stream_length_matches_content(), patched_text_is_winansi_not_utf8(), replace_text_paints_the_requested_colours(), signature_does_not_clobber_page_font(), signature_label_cannot_inject_operators()

### Community 155 - "Config Module"
Cohesion: 0.44
Nodes (7): aller_retour_sur_les_quatre_rotations(), assert_close(), assert_close2(), identite_sans_rotation(), recadrage_borne_la_fraction(), rotation_quatre_vingt_dix(), view()

### Community 156 - "App Module"
Cohesion: 0.22
Nodes (4): Agent A2 — Page Resize, Interleaving, Page Exchange, Agent B2 — Page Resize & Exchange Dialog, Agent D2 — Whole-PDF Bilingual Translation, Wave 4 — AI Gaps (2 Parallel Agents)

### Community 157 - "Common Module"
Cohesion: 0.25
Nodes (7): docs/CONTRIBUTING.md, docs/GAP.md, reference notes, YouPDF — Parity Agent Specifications (Full Detail), UPDF 2.5.6, docs/visual-parity.md, Wave 0 — Collision Prep

### Community 158 - "Link Appearance Module"
Cohesion: 0.22
Nodes (9): Batch Processing Mode, Dark Theme, Drag-and-Drop File Upload ("or drop a file anywhere"), File Search Input, YouPDF Home Screen, Local-first PDF Editing Value Proposition, Open File Button, Icon Sidebar Navigation (+1 more)

### Community 159 - "PARITY AGENT SPECS Module"
Cohesion: 0.17
Nodes (12): defect_label(), detect_page_defects(), detecte_blank_et_dimensions(), detecte_rotation_different(), PageDefect, Blank, ContainsComments, Invalid (+4 more)

### Community 160 - "Youpdf Doc Module"
Cohesion: 0.29
Nodes (3): SignUi, CertificateRequest, dn_value()

### Community 162 - "OfficeFormat"
Cohesion: 0.20
Nodes (6): Document, OfficeFormat, .ALL, Excel, PowerPoint, Word

### Community 163 - "src/util.rs"
Cohesion: 0.16
Nodes (17): escape_html(), escape_markdown(), escape_rtf(), ampersand_unescapes_last(), csv_escape(), csv_quotes_only_when_needed(), escape_xml(), flate_compress() (+9 more)

### Community 164 - "Page Resize Module"
Cohesion: 0.20
Nodes (10): main(), RedactionRegion, fold_case(), annots_of(), doc_with_annots(), fold_case_preserves_byte_length(), ocr_language_must_be_a_language_code(), redaction_preserves_annotations_outside_the_redacted_area() (+2 more)

### Community 165 - "Pdf Dates Module"
Cohesion: 0.50
Nodes (6): exchange_disjoint_ranges(), exchange_overlapping_ranges_rejected(), interleave_even_first(), interleave_odd_first(), order(), reload()

### Community 166 - "Word Count Module"
Cohesion: 0.25
Nodes (3): Agent A1 — Page Labels, Agent B1 — Page Label Editor, Agent D1 — AI Writing Tools & AI Check Pages

### Community 168 - "YouPdfApp"
Cohesion: 0.24
Nodes (4): encode_png(), raster_font(), unscaled_em(), YouPdfApp

### Community 172 - "App Module"
Cohesion: 0.29
Nodes (3): Agent A5 — Word Count & Document Stats, Agent A6 — Security Permissions Matrix, Agent B5 — Properties Window Stats & Permissions

### Community 182 - "STATUS Module"
Cohesion: 0.33
Nodes (5): F2 — stale STATUS / parity tracker docs, F1 — cargo test --workspace fails (pdfium unavailable), Wave verification gate (check/test/clippy/i18n), Test suite result — 414 tests, 0 failures, Authoritative verification commands

### Community 183 - "AUDIT REPORT Module"
Cohesion: 0.33
Nodes (6): Element Align/Arrange Panel (deferred), LittleCMS, Office Import (DONE) — soffice shell-out, Validating PDF/A (deferred) — PDF/A-2b scope, PDF Portfolio (deferred) — /Collections wrapper, Wave 5 — Strategic Product Decisions (No Code Agents)

### Community 187 - "AUDIT STATUS Module"
Cohesion: 0.50
Nodes (4): AttachmentAction, Delete, Open, SaveAs

### Community 190 - "AUDIT REPORT Module"
Cohesion: 0.50
Nodes (4): UPDF reference material, Feature inventory spec, November 2026 reference note, Evidence map

### Community 191 - "CONTRIBUTING Module"
Cohesion: 0.50
Nodes (3): Phased release plan (v0.1 Viewer – v0.6 Stretch), Cargo workspace crates, YouPDF — free clean-room Rust PDF editor

### Community 192 - "PLAN Module"
Cohesion: 0.50
Nodes (4): Empty Recent Files State, Recents Tab, Starred Tab, Tools Tab

### Community 193 - "Visual Parity Module"
Cohesion: 0.83
Nodes (4): youpdf-ai, youpdf-app, youpdf-core, youpdf-shared

### Community 195 - "Build Macos App Module"
Cohesion: 0.67
Nodes (3): Dark Squircle Background, Gradient Y Monogram, YouPDF App Icon

### Community 196 - "Build Macos Icon Module"
Cohesion: 0.67
Nodes (3): YouPDF Application Icon — a dark navy rounded-square (squircle) app icon containing a centered, thick rounded gradient Y monogram; the standard brand/app-store icon for YouPDF, Gradient Squircle Icon Style — modern macOS/iOS-style rounded-square icon with near-black navy background and a vibrant cyan-to-purple gradient, giving a glossy, contemporary brand aesthetic, Y Monogram Logo Mark — stylized lowercase 'Y' rendered as one continuous smooth rounded ribbon; left arm cyan/teal, right arm magenta/purple, stem blue-purple, with a soft translucent glow where the arms cross

### Community 197 - "Build Windows Zip Module"
Cohesion: 0.67
Nodes (3): C1 — YouPdfApp ~518-line, ~283-field god struct, Phase 5 refactor — app.rs split into 20+ panel modules, Tier 1 parity status — 7 Closed, 2 Partial

### Community 199 - "Icon Module"
Cohesion: 0.67
Nodes (3): UPDF competitor comparison (Adobe, PDF Expert, Foxit), UPDF free-version limits, UPDF pricing / freemium model

## Knowledge Gaps
- **507 isolated node(s):** `Aes256`, `Rc4_128`, `Move`, `Rotate`, `Segment` (+502 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 1229 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **121 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Document` connect `Document` to `YouPDF UI Constants and Themes`, `PDF Encryption and Crypto`, `blank_document`, `forms_inherited.rs`, `Links Module`, `Smoke Module`, `convert_export.rs`, `Inspector Panel`, `.toolbar`, `Image Compression`, `App Module`, `Batch Types Module`, `Ai Module`, `Release Module`, `tests/crop.rs`, `Theme Module`, `Mod Module`, `Mod Module`, `Matrix Module`, `Measure Module`, `Labels Module`, `PARITY AGENT SPECS Module`, `Comments Ui Module`, `Visual Parity Module`, `link_appearance.rs`, `src/util.rs`, `Page Resize Module`, `Pdf Dates Module`, `TextFormat`, `Theme Module`, `Keyring Module`, `Forms Ui Module`, `outline.rs`, `Create Module`, `Office Export Module`, `Outline Module`, `Object`, `Dictionary`, `watermark/mod.rs`, `Organize Dialogs Module`, `Write Module`, `Registry Module`, `src/create.rs`, `Result`, `Write Module`, `Endpoint Module`, `Message Module`, `batch_types.rs`, `Burn Module`, `Powerpoint Module`, `Pdfium Module`, `Mod Module`, `Ollama Module`, `Sign Ui Module`, `.touch`, `Edit Style Module`, `Identity Module`, `blank_document`, `Comment Module`, `Image Module`, `Organize Ui Module`, `Impl Pdfium Module`, `Render Cache Module`, `redact.rs`, `Outline Gen Module`, `PARITY AGENT SPECS Module`, `edit/mod.rs`?**
  _High betweenness centrality (0.262) - this node is a cross-community bridge._
- **Are the 6 inferred relationships involving `Document` (e.g. with `.from_lopdf()` and `read_permissions()`) actually correct?**
  _`Document` has 6 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Aes256`, `Rc4_128`, `Move` to the rest of the system?**
  _507 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `YouPDF UI Constants and Themes` be split into smaller, more focused modules?**
  _Cohesion score 0.018907563025210083 - nodes in this community are weakly interconnected._
- **Why does `YouPdfApp` connect `YouPdfApp` to `YouPDF UI Constants and Themes`, `Links Module`, `PDF Encryption and Crypto`, `Youpdf View Module`, `App Dialogs and Windows`, `Icon Glyph Library`, `PARITY AGENT SPECS Module`, `Menu Action Enum`, `.toolbar`, `Image Compression`, `Audit Fixes Module`, `Batch Types Module`, `pal`, `ImageFormat`, `Mod Module`, `Matrix Module`, `Labels Module`, `HeaderFooter`, `Youpdf Doc Module`, `.toast`, `ai.rs`, `OfficeFormat`, `Ops Module`, `TextFormat`, `ai.rs`, `Office Export Module`, `Outline Module`, `Dictionary`, `watermark/mod.rs`, `Types Module`, `Endpoint Module`, `Message Module`, `batch_types.rs`, `home.rs`, `Burn Module`, `Attachments Module`, `Pdfa Module`, `.touch`, `Render Module`, `organize_ui.rs`, `redact.rs`, `Youpdf Modepill Module`, `edit/mod.rs`?**
  _High betweenness centrality (0.057) - this node is a cross-community bridge._
- **Are the 72 inferred relationships involving `t()` (e.g. with `.body()` and `.show()`) actually correct?**
  _`t()` has 72 INFERRED edges - model-reasoned connections that need verification._
- **Should `PDF Encryption and Crypto` be split into smaller, more focused modules?**
  _Cohesion score 0.05249569707401033 - nodes in this community are weakly interconnected._