// SPDX-License-Identifier: GPL-3.0-or-later
//! Builds the QML module and the cxx-qt objects, and compiles the translations (`i18n/*.ts`, Qt
//! Linguist files) with Qt's `lrelease` into `.qm` files that the crate embeds.

use std::path::{Path, PathBuf};
use std::process::Command;

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    compile_translations();

    let qml = [
        QmlFile::from("qml/Main.qml"),
        QmlFile::from("qml/Theme.qml").singleton(true),
        QmlFile::from("qml/Icons.qml").singleton(true),
        QmlFile::from("qml/AppIcon.qml"),
        QmlFile::from("qml/AppIconLabel.qml"),
        QmlFile::from("qml/AppBanner.qml"),
        QmlFile::from("qml/AppButton.qml"),
        QmlFile::from("qml/AppCheckBox.qml"),
        QmlFile::from("qml/AppComboBox.qml"),
        QmlFile::from("qml/AppItemDelegate.qml"),
        QmlFile::from("qml/AppProgressBar.qml"),
        QmlFile::from("qml/AppRatingMark.qml"),
        QmlFile::from("qml/AppRadioButton.qml"),
        QmlFile::from("qml/AppSlider.qml"),
        QmlFile::from("qml/AppTextArea.qml"),
        QmlFile::from("qml/AppTextField.qml"),
        QmlFile::from("qml/AppToolButton.qml"),
        QmlFile::from("qml/AppShadow.qml"),
        QmlFile::from("qml/AppTabButton.qml"),
        QmlFile::from("qml/AppSpinBox.qml"),
        QmlFile::from("qml/AppListFrame.qml"),
        QmlFile::from("qml/AppListView.qml"),
        QmlFile::from("qml/AppScrollBar.qml"),
        QmlFile::from("qml/AppDialog.qml"),
        QmlFile::from("qml/AppDialogButtonBox.qml"),
        QmlFile::from("qml/AppWindow.qml"),
        QmlFile::from("qml/AppActions.qml"),
        QmlFile::from("qml/AppMenu.qml"),
        QmlFile::from("qml/AppSubMenu.qml"),
        QmlFile::from("qml/AppMenuItem.qml"),
        QmlFile::from("qml/NoticeBar.qml"),
        QmlFile::from("qml/SettingsDialog.qml"),
        QmlFile::from("qml/AboutDialog.qml"),
        QmlFile::from("qml/Catalogue.qml"),
        QmlFile::from("qml/CatalogueFlow.qml"),
        QmlFile::from("qml/AddSourceDialog.qml"),
        QmlFile::from("qml/RemoveSourceDialog.qml"),
        QmlFile::from("qml/RestoreDialog.qml"),
        QmlFile::from("qml/FolderPicker.qml"),
        QmlFile::from("qml/ImportDialog.qml"),
        QmlFile::from("qml/CardBanner.qml"),
        QmlFile::from("qml/Welcome.qml"),
        QmlFile::from("qml/NewWorkspaceDialog.qml"),
        QmlFile::from("qml/Library.qml"),
        QmlFile::from("qml/KeywordPanel.qml"),
        QmlFile::from("qml/MetadataPanel.qml"),
        QmlFile::from("qml/InfoPanel.qml"),
        QmlFile::from("qml/CollectionPanel.qml"),
        QmlFile::from("qml/Viewer.qml"),
        QmlFile::from("qml/Histogram.qml"),
        QmlFile::from("qml/ComparePane.qml"),
        QmlFile::from("qml/Compare.qml"),
        QmlFile::from("qml/SimilarPanel.qml"),
        QmlFile::from("qml/FileSaveDialog.qml"),
        QmlFile::from("qml/DuplicatesDialog.qml"),
        QmlFile::from("qml/ExternalBanner.qml"),
        QmlFile::from("qml/ExternalChangesDialog.qml"),
        QmlFile::from("qml/XmpExportDialog.qml"),
    ];
    let mut builder = CxxQtBuilder::new_qml_module(QmlModule::new("org.auroraw.ui").qml_files(qml))
        .qt_module("Quick")
        // The icon font, at qrc:/qt/qml/org/auroraw/ui/assets/fonts/AurorawIcons.ttf (`Icons.qml` loads it).
        .qrc_resources(["assets/fonts/AurorawIcons.ttf"])
        .files([
            "src/bus.rs",
            "src/duplicates.rs",
            "src/external_changes.rs",
            "src/files.rs",
            "src/folders.rs",
            "src/history.rs",
            "src/import_form.rs",
            "src/models.rs",
            "src/shortcuts.rs",
            "src/launcher.rs",
            "src/xmp_export.rs",
        ])
        .cpp_file("src/glue.cpp");
    if std::env::var_os("CARGO_FEATURE_QUICKTEST").is_some() {
        builder = builder.qt_module("QuickTest").cpp_file("src/quicktest.cpp");
    }
    builder = with_utf8_sources(builder);
    builder.build();
}

/// MSVC reads a source file as the legacy code page unless told it is UTF-8, and the C++ generated from
/// the QML holds its strings (the rating's star, the ellipses) as UTF-8 text: without this flag they
/// come out as mojibake on Windows.
#[allow(unsafe_code)] // cxx-qt-build marks `cc_builder` unsafe; this only adds a compiler flag.
fn with_utf8_sources(builder: CxxQtBuilder) -> CxxQtBuilder {
    // SAFETY: the callback only adds a flag to the C++ compiler's command line.
    unsafe {
        builder.cc_builder(|cc| {
            if cc.get_compiler().is_like_msvc() {
                cc.flag("/utf-8");
            }
        })
    }
}

/// The directory of Qt's command line tools, from the `qmake` the cxx-qt build uses.
fn qt_bins() -> PathBuf {
    let qmake = std::env::var("QMAKE").unwrap_or_else(|_| {
        if Command::new("qmake6").arg("-v").output().is_ok() {
            "qmake6".into()
        } else {
            "qmake".into()
        }
    });
    let out = Command::new(&qmake)
        .args(["-query", "QT_INSTALL_BINS"])
        .output()
        .unwrap_or_else(|e| panic!("cannot run `{qmake} -query QT_INSTALL_BINS`: {e}"));
    PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
}

fn compile_translations() {
    println!("cargo:rerun-if-env-changed=QMAKE");
    println!("cargo:rerun-if-changed=i18n");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let bins = qt_bins();
    let lrelease = ["lrelease", "lrelease.exe"]
        .iter()
        .map(|name| bins.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| {
            panic!(
                "Qt's lrelease is not in {} (install Qt's tools: qt6-l10n-tools on Debian and \
                 Ubuntu, the qttools module with aqtinstall)",
                bins.display()
            )
        });

    let mut table = String::from("/// The compiled translations: language code and `.qm` bytes.\n");
    table.push_str("pub const TRANSLATIONS: &[(&str, &[u8])] = &[\n");
    let mut sources: Vec<PathBuf> = std::fs::read_dir("i18n")
        .expect("the i18n folder")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "ts"))
        .collect();
    sources.sort();
    for source in sources {
        println!("cargo:rerun-if-changed={}", source.display());
        let stem = source.file_stem().unwrap().to_string_lossy().into_owned();
        let code = stem.strip_prefix("auroraw_").unwrap_or(&stem).to_string();
        let qm = out_dir.join(format!("{stem}.qm"));
        let status = Command::new(&lrelease)
            .arg("-silent")
            .arg(&source)
            .arg("-qm")
            .arg(&qm)
            .status()
            .expect("lrelease runs");
        assert!(status.success(), "lrelease failed on {}", source.display());
        table.push_str(&format!(
            "    ({code:?}, include_bytes!({:?})),\n",
            path_text(&qm)
        ));
    }
    table.push_str("];\n");
    std::fs::write(out_dir.join("translations.rs"), table).expect("translations.rs");
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
