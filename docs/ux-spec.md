# YouPDF — UPDF UX Inventory

Target: `/Applications/UPDF.app` v2.5.6, native Swift/ObjC, 270 MB, bundle `com.superace.updf.mac`.
Locale: French — all strings below are UPDF's French UI. EN equivalents in parentheses where useful.
The full menu dump is next to this file in `menus-fr.txt`.

Clean-room rule: this spec records layout, labels and behavior only. No UPDF assets, icons or code go into YouPDF.

## 1. Window chrome

- Window: 1470×923 pt default, standard traffic lights, dark titlebar.
- Row 1 (height ≈ 40 pt): `Home` house button (left) · tab strip: file tabs (title + ✕) · `+` new tab · right side: 🔔 notifications · avatar `U`.
- Window title = active tab name.

## 2. Left icon rail (~52 pt wide, 8 buttons top-to-bottom)

| # | FR label (tooltip) | Icon |
|---|---|---|
| 1 | Organiser les pages (Organize Pages) | 4-square grid |
| 2 | Rechercher (Search) | magnifier |
| 3 | Vignettes (Thumbnails) | framed page |
| 4 | Signets (Bookmarks) | bookmark ribbon |
| 5 | Commentaires (Comments) | speech bubble |
| 6 | Champs de formulaire (Form Fields) | form grid |
| 7 | Pièces jointes (Attachments) | pen/attach glyph |
| 8 | Aspect (Appearance) | box glyph (bottom) |

Rail button = panel switcher, one at a time. Active button tinted accent. Panel width ≈ 200 pt, resizable via splitter.

## 3. Side panels

**Rechercher** (`shot-02-search.png`, `state-04.png`)
- Header `Rechercher` + funnel (Filtrer) button.
- 3 icon actions row: search text · `Aa` find&replace · trash find&delete · redact-glyph find&redact.
- Input `Rechercher du Texte`.
- `Historique` header + clear-trash. History rows: query text + ✕ remove (seen: "ecart-type", "ecart", "ecart", "disque").
- AX: Filtrer / Rechercher du Texte / Rechercher du texte et remplacer / Rechercher du texte et supprimer / Rechercher du texte et des biffures / Effacer la liste.

**Vignettes**: table of page thumbnails, page number under each, selected page tinted `#E8E8F8`.

**Signets / Commentaires / Champs de formulaire / Pièces jointes**: same panel slot, list style. Empty for our sample.

**Aspect**: button present; opens no AX-visible panel in this build (menu « Affichage → Thème / Arrière-plan » covers themes: Light, Dark, Starry Blue, Mint Green, Auto + 9 page backgrounds).

## 4. Document toolbar (view mode)

- Left, above canvas: `Outils` dropdown (opens tool list).
- Center pill: 4 mode tabs with icon + label: `Commentaire` · `Modifier` · `Formulaire` · `Biffer`.
- Right: `Annuler` (undo) · `Rétablir` (redo) · `…` overflow (contains Enregistrer, Enregistrer sous + more).
- Mode switch also via menu Outils: ⌃1 Commentaire, ⌃2 Modifier le fichier PDF, ⌃3 Préparer le formulaire, ⌃4 Biffer, ⌃5 Organiser les pages, ⌃6 Rogner, ⌃7 Séparer, ⌃8 Filigrane, ⌃9 Arrière-plan, ⌃0 En-tête/pied.

## 5. Mode toolbars (floating pill over canvas, all end with `Fermer`)

**Commentaire — annotation** (`state-07-annot.png`)
select • highlight • text • comment-box • ink • shapes (rect/ellipse/arrow/line) • insert(+) • link • emoji/stamp • signature • eraser → `Fermer`.
Every tool has a chevron `▾` for presets (color/width).

**Modifier — edit** (`state-09-edit.png`)
select • `Texte ▾` • `Image ▾` • `Lien ▾` • (layout/pin icon) → `Fermer`.

**Formulaire — form builder** (`state-10-form.png`)
select • text field • checkbox • radio • dropdown • list · button • date • barcode • signature • image • divider • `••• Autres` → `Fermer`.

**Biffer — redact** (`state-11-redact.png`, `state-12-redact-clean.png`)
select • `Biffer les pages` • `Rechercher du texte et des biffures` • `Assainir le document` • `Appliquer` (accent) → `Fermer`.
First entry shows onboarding modal: illustration + "La biffure permet de biffer et de supprimer définitivement le contenu sensible" + two steps (1. Repérez les biffures. 2. Appliquez les biffures.) + note that redactions are permanent after Apply + `Ne plus afficher` checkbox + `OK`.

**Organiser les pages — full-window page manager** (`shot-03-organize.png`)
Row: `?` help • `Sélection multiple` (toggle) • `Sélection des pages ▾` • `Insérer ▾` • `Extraire` • `Remplacer` • `Fractionner ▾` • `••• Autres` • rotate-CCW • rotate-CW • trash • `Fermer`. Right: undo/redo/overflow.
Canvas: page tiles (page no. + size label e.g. `Lettre (215.9 × 279.4 mm)`), selection border accent, bottom-right zoom −/+.
Exit = `Fermer` (Escape does NOT exit).

## 6. Theme palette (measured from renders)

| Role | Hex |
|---|---|
| Page / document | `#FFFFFF` |
| Canvas background | `#303030` |
| Titlebar / tab row | `#282828` |
| Buttons, hover surfaces | `#383838` |
| Accent (selected, primary) | `#7A7EF6` |
| Selected thumbnail tint | `#E8E8F8` |
| Muted text/icons | `#A0A0A0` |

Window chrome: dark, rounded canvases, floating pills with soft shadows, icon+label buttons, accent = periwinkle indigo.

## 7. Menu bar (full dump: `menus-fr.txt`)

- **UPDF**: À propos, Rechercher les mises à jour, Paramètres ⌘, Services, Masquer, Quitter.
- **Fichier**: Nouvel onglet ⌘T · Nouvelle fenêtre · Créer (blank, from file/images/scanner/Word/Excel/PPT/CAJ, screen capture ×3, clipboard, Portfolio PDF) · Ouvrir ⌘O · Ouvrez rapidement ⇧⌘F · Récent · Fermer onglet ⌘W · Fermer fenêtre ⇧⌘W · Enregistrer ⌘S / sous ⇧⌘S + (réduire taille, aplatir, PDF/A-/X-/E, PDF d'image) · Exporter vers (Word, PowerPoint, Excel, Image, Texte, RTF, CSV, XML, HTML) · UPDF Cloud · Mot de passe (protéger, enlever, propriétés) · Afficher dans Finder ⇧⌘J · Propriétés ⌘D · Imprimer ⌘P.
- **Modifier**: standard + Sélectionner tous les commentaires ⇧⌘A · Remplir (contacts/mots de passe/carte) · Dictée · Emoji.
- **Affichage**: Rechercher (⌘F, suivant ⌘G, précédent ⇧⌘G, remplacer ⇧⌘F, biffures ⇧⌘R) · panels Vignettes/Signets/Commentaires/Champs/PJ (⌘⇧1..5) · Thème · Arrière-plan & Affichage · Masquer la barre d'outils · Affichage de page (1 page, 1 page défil., 2 pages, 2 pages défil., cover option) · Zoom (avant, arrière, taille réelle ⌘7, pleine largeur ⌘8, pleine hauteur ⌘9, page entière ⌘0) · Diaporama (avec vue Présentateur) · Plein écran.
- **Outils**: ROC (OCR) · Convertir (12 formats + PDF/A-/X-/E) · Créer (dup de Fichier) · Traitement par lots (convertir ⇧⌘B, OCR, combiner, insérer, créer, filigrane, arrière-plan, en-tête/pied, Bates, compresser, chiffrer, enlever, imprimer) · modes ⌃1..⌃0 (voir §4) · Comparer des PDF · Protéger · Réduire la taille · Aplatir.
- **Aller à**: première/dernière page, page précédente/suivante, accéder à la page ⇧⌘G, aller/revenir à la page %d.
- **Tab**, **Fenêtre**, **Aide** (Acheter, Voir mon compte, Page du produit, Guide, Licence).

Shortcut modifiers in `menus-fr.txt` are raw AX values: `mod=8` = no-Command, `12` = ⌃, `1` = ⇧, `2` = ⌥, `10` = ⌥+no-⌘.

## 8. Behaviors worth copying

- Modes are modal contexts: entering swaps the center toolbar; `Fermer` exits; rail stays usable.
- Long operations (redact) use two-phase UX: mark → `Appliquer`, explicit warning modal on first use.
- Organize Pages is a separate full-window surface with its own toolbar, not a dialog.
- Everything undoable from the same top-right cluster; save is explicit (`Enregistrer`) + Save As.
- Localization FR/EN; strings short (1-2 words) for toolbar labels.

## 9. Gaps / not inventoried

- Home/start page (house button) — not captured.
- Aspect panel content; annotation preset palettes (color pickers); Settings window; export dialogs; OCR/batch dialogs; password dialogs.
- Light theme rendering (menu shows Light/Dark themes).

## 10. Cross-check with Target_Job.md / UPDF_Deep.md

- `Target_Job.md` pre-sketch (tool tabs Lecteur/Édition/Annotation/…/IA/Signer, left tool sidebar, right contextual panel, status bar with provider indicator) describes a *simpler* chrome than UPDF's actual rail+floating-pill layout. Resolution for YouPDF: keep UPDF-like rail + mode tabs structure (this spec) as the interaction model, add the brief's right contextual panel and status bar (page · zoom · lang · local-lock · AI provider). This satisfies both: usage logic replicated, original identity.
- `Target_Job.md` §12 forbids pixel-identical UI and copied assets — matches our boundary. So "same look" = same layout logic, palette family, motion; icons/art/strings stay ours.
- `UPDF_Deep.md` differentiators to bake in: no watermark ever, offline-first, small optional OCR packs (vs UPDF's 1.4-1.7 GB plugin), Linux support, no cloud dependency, working non-Latin fonts.
- UPDF free tier adds watermarks on save — YouPDF must never do that (already in plan).

