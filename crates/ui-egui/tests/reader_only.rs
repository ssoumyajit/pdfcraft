#![cfg(feature = "reader-only")]

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::{LeftPanel, Mode, PdfCraftApp};

/// A tiny PDF with two pages, two bookmarks and one sticky note.
const FIXTURE: &[u8] = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R /Outlines 6 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Annots [9 0 R] >> endobj
4 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >> endobj
6 0 obj << /Type /Outlines /First 7 0 R /Last 8 0 R /Count 2 >> endobj
7 0 obj << /Title (Alpha section) /Parent 6 0 R /Next 8 0 R /Dest [3 0 R /Fit] >> endobj
8 0 obj << /Title (Beta section) /Parent 6 0 R /Prev 7 0 R /Dest [4 0 R /Fit] >> endobj
9 0 obj << /Type /Annot /Subtype /Text /Rect [10 370 30 390] /T (Tester) /Contents (Check the numbers) >> endobj
trailer << /Root 1 0 R >>
%%EOF";

#[test]
fn reader_only_shell_stays_in_read_mode() {
    let mut app = PdfCraftApp::new();
    assert_eq!(app.mode, Mode::Read);
    assert_eq!(app.default_mode, Mode::Read);
    assert!(!app.left_open);

    // Old persisted settings and automation options cannot restore the removed
    // workspaces or their tool panels in the embedded reader.
    app.restore(r#"{"default_mode":"edit"}"#);
    assert_eq!(app.default_mode, Mode::Read);
    app.set_option("mode", "convert").expect("mode option is safely normalized");
    assert_eq!(app.mode, Mode::Read);
    assert_eq!(app.left, LeftPanel::AllTools);
    assert!(!app.left_open);
    assert!(app.set_option("tool", "export").is_err());
}

#[test]
fn reader_only_read_mode_renders_right_rail_panels() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.open_bytes("fixture.pdf", None, FIXTURE.to_vec()).expect("fixture opens");
        app
    });
    // Fonts install on frame 1 and apply on frame 2; allow rendering to settle.
    h.run_steps(4);

    // The document opens with its Comments panel, then rail navigation can move
    // to another reader-side panel while remaining in Read mode.
    h.get_by_label_contains("Check the numbers");
    h.get_by_label("Bookmarks").click();
    h.run_steps(3);
    h.get_by_label("Alpha section");
}

#[test]
fn reader_only_home_is_an_unbranded_blank_canvas() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| PdfCraftApp::new());
    h.run_steps(4);

    assert_eq!(h.query_all_by_label("Welcome to PdfCraft").count(), 0);
    assert_eq!(h.query_all_by_label("Join the ArtCraft community").count(), 0);
    assert_eq!(h.query_all_by_label("Recommended tools").count(), 0);
    assert_eq!(h.query_all_by_label("Open file").count(), 0);
}
