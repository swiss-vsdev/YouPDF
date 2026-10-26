use youpdf_core::protect::{permissions_from_flags, permissions_from_struct, Permissions};

#[test]
fn decode_print_flag() {
    let rows = permissions_from_flags(0x0004);
    let print = rows.iter().find(|r| r.key == "Print").unwrap();
    assert!(print.allowed);
    let modify = rows.iter().find(|r| r.key == "Modify").unwrap();
    assert!(!modify.allowed);
}

#[test]
fn all_allowed() {
    let rows = permissions_from_flags(0x0FFF);
    assert!(rows.iter().all(|r| r.allowed));
}

#[test]
fn none_allowed() {
    let rows = permissions_from_flags(0x0000);
    assert!(rows.iter().all(|r| !r.allowed));
}

#[test]
fn roundtrip_with_struct() {
    let p = Permissions::all();
    let rows = permissions_from_struct(&p);
    assert!(rows.iter().all(|r| r.allowed));
}
