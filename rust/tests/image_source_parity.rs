//! Opt-in local-image verification; no console content is stored in this test.

use clover_portable::image_reader::{read_image, read_image_from};
use sha2::{Digest, Sha256};
use std::{fs, io::Cursor, path::PathBuf};

#[test]
#[ignore = "requires CLOVER_SOURCE_PARITY_IMAGE pointing to a local image"]
fn local_image_is_identical_through_path_and_borrowed_reader() {
    let path = PathBuf::from(
        std::env::var_os("CLOVER_SOURCE_PARITY_IMAGE")
            .expect("set CLOVER_SOURCE_PARITY_IMAGE to a local filesystem image"),
    );
    let original = fs::read(&path).unwrap();
    let before = Sha256::digest(&original);
    let expected = read_image(&path).unwrap();
    let actual = read_image_from(Cursor::new(original.as_slice()), original.len() as u64).unwrap();
    assert!(
        !expected.is_empty(),
        "use an image containing selected files"
    );
    assert!(actual == expected, "selected file names or bytes differ");
    assert_eq!(Sha256::digest(fs::read(path).unwrap()), before);
}
