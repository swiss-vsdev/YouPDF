//! Security permissions matrix: decode the `/P` integer from an encrypted
//! PDF's `/Encrypt` dictionary into a typed, human-readable matrix.
//!
//! The encoding side already exists on `Permissions::to_pdf_flags()`.

use crate::protect::Permissions;

/// A single permission row for the Properties window UI.
#[derive(Debug, Clone, Copy)]
pub struct PermissionRow {
    pub key: &'static str,        // e.g. "Print"
    pub label: &'static str,      // e.g. "Printing"
    pub allowed: bool,
}

/// Decode a `/P` integer into a list of permission rows.
///
/// Bit mapping (PDF 32000-1 §7.5.4):
///   bit 3 (0x0004) = Print
///   bit 4 (0x0008) = Modify
///   bit 5 (0x0010) = Copy content
///   bit 6 (0x0020) = Add/modify annotations
///   bit 9 (0x0100) = Fill forms
///   bit 10 (0x0200) = Extract for accessibility
///   bit 11 (0x0400) = Assemble document
///   bit 12 (0x0800) = Print at high quality
pub fn permissions_from_flags(p: i32) -> Vec<PermissionRow> {
    vec![
        PermissionRow { key: "Print", label: "Printing", allowed: p & 0x0004 != 0 },
        PermissionRow { key: "Modify", label: "Modifying the document", allowed: p & 0x0008 != 0 },
        PermissionRow { key: "Copy", label: "Copying content", allowed: p & 0x0010 != 0 },
        PermissionRow { key: "Annotate", label: "Adding annotations", allowed: p & 0x0020 != 0 },
        PermissionRow { key: "FillForms", label: "Filling forms", allowed: p & 0x0100 != 0 },
        PermissionRow { key: "Accessibility", label: "Extracting for accessibility", allowed: p & 0x0200 != 0 },
        PermissionRow { key: "Assemble", label: "Assembling the document", allowed: p & 0x0400 != 0 },
        PermissionRow { key: "PrintHighQuality", label: "Printing at high quality", allowed: p & 0x0800 != 0 },
    ]
}

/// Read the `/P` permission integer from a document's `/Encrypt` dictionary.
/// Returns `None` if the document is not encrypted.
pub fn read_permissions(doc: &crate::document::Document) -> Option<Vec<PermissionRow>> {
    // The `/Encrypt` entry lives in the trailer as a reference to a dictionary
    // object. Resolve it, then pull `/P` (an Integer) out of that dictionary.
    let inner = doc.inner();
    let encrypt_ref = inner.trailer.get(b"Encrypt").ok()?;
    let encrypt_id = encrypt_ref.as_reference().ok()?;
    let encrypt_obj = inner.get_object(encrypt_id).ok()?;
    let encrypt_dict = encrypt_obj.as_dict().ok()?;
    let p = encrypt_dict.get(b"P").ok()?.as_i64().ok()? as i32;
    Some(permissions_from_flags(p))
}

/// Convenience: decode flags into a `Permissions` struct (re-exports the
/// existing struct for UI convenience).
pub fn permissions_from_struct(p: &Permissions) -> Vec<PermissionRow> {
    permissions_from_flags(p.to_pdf_flags())
}

// ponytail: No UI for CreateTemplatePages/HighQualityPrint (rare); add if enterprise workflows need it.