// SPDX-License-Identifier: GPL-3.0-or-later
//! A photo's file, resolved from the catalogue (WP9, D-106): "show in file manager" and exporting the listed
//! photos both need it, and neither is a `Command` (read-only, like `sources_api` and `similar_api`). Reuses the
//! lookup `thumbnails::source_root` already does for its own worker, but reads every source's root **once** for a
//! whole batch, since exporting a long list would otherwise reread `sources.json` once per photo.

use std::collections::HashMap;
use std::path::PathBuf;

use auroraw_types::{PhotoId, SourceId};

use crate::Engine;
use crate::error::Result;
use crate::thumbnails::source_root;

impl Engine {
    /// Every root a source in this workspace resolves to on this machine, by identifier: sources with no root (not
    /// registered on this machine, or offline) are simply absent.
    fn source_roots(&self) -> HashMap<SourceId, PathBuf> {
        let Ok(Some(sources)) = self.workspace().read_sources() else {
            return HashMap::new();
        };
        let Some(current) = sources.current() else {
            return HashMap::new();
        };
        current
            .sources
            .iter()
            .filter_map(|s| Some((s.id, source_root(self.workspace(), s.id)?)))
            .collect()
    }

    /// Each of `ids`' file, in the same order, `None` for a photo whose source has no root here, or that has no
    /// location at all (never read: the catalogue is opened once, and every source's root resolved once, for the
    /// whole batch).
    pub fn original_paths(&self, ids: &[PhotoId]) -> Result<Vec<Option<PathBuf>>> {
        let catalogue = self.read_catalogue()?;
        let roots = self.source_roots();
        Ok(ids
            .iter()
            .map(|id| {
                let row = catalogue.photo(id).ok().flatten()?;
                let root = roots.get(&row.source_id?)?;
                Some(root.join(row.path?.replace('/', std::path::MAIN_SEPARATOR_STR)))
            })
            .collect())
    }

    /// One photo's file, `None` on the same terms as [`Self::original_paths`].
    pub fn original_path(&self, id: PhotoId) -> Result<Option<PathBuf>> {
        Ok(self.original_paths(std::slice::from_ref(&id))?.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use auroraw_format::state::Sources;
    use auroraw_types::Timestamp;

    use super::*;
    use crate::{Command, Engine, Outcome};

    fn write_jpeg(path: &std::path::Path, seed: u8) {
        use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(16, 16, |x, y| {
            Rgb([(x as u8) ^ seed, (y as u8) ^ seed, seed])
        }))
        .save_with_format(path, ImageFormat::Jpeg)
        .unwrap();
    }

    #[test]
    fn a_photos_file_resolves_to_its_source_and_a_photo_of_no_source_or_an_offline_one_does_not() {
        let dir = auroraw_testkit::temp_dir();
        let (engine, events) = Engine::create(
            &dir.path().join("Main"),
            &dir.path().join("main.sqlite"),
            "Main",
        )
        .unwrap();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();
        write_jpeg(&card.join("a.jpg"), 1);
        write_jpeg(&card.join("b.jpg"), 2);
        let Outcome::SourceAdded(source_id) = engine
            .submit_and_wait(Command::AddSource {
                name: "Card".into(),
                root: card.clone(),
                kind: auroraw_sources::filesystem::LOCAL_FOLDER.into(),
            })
            .unwrap()
        else {
            panic!("expected SourceAdded");
        };
        let Outcome::Scanned { new, .. } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!("expected Scanned");
        };
        let Outcome::PhotosAdded(added) = engine
            .submit_and_wait(Command::AddNewPhotos {
                source_id,
                paths: new,
            })
            .unwrap()
        else {
            panic!("expected PhotosAdded");
        };
        assert_eq!(added.len(), 2);
        let mut rows = engine
            .read_catalogue()
            .unwrap()
            .list_recent(None, 10)
            .unwrap();
        rows.sort_by(|a, b| a.filename.cmp(&b.filename));
        let ids: Vec<PhotoId> = rows.iter().map(|r| r.id).collect();

        let resolved = engine.original_paths(&ids).unwrap();
        assert_eq!(
            resolved,
            [Some(card.join("a.jpg")), Some(card.join("b.jpg"))]
        );
        assert_eq!(
            engine.original_path(ids[0]).unwrap(),
            Some(card.join("a.jpg"))
        );

        // A photo of no known source has no file.
        assert_eq!(engine.original_path(PhotoId::random()).unwrap(), None);

        // The source's own root is gone from the workspace's hint (as if unplugged and its hint lost): offline, so
        // no file either.
        engine
            .workspace()
            .write_sources(&Sources {
                updated: Timestamp::now(),
                sources: Vec::new(),
                extra: Default::default(),
            })
            .unwrap();
        assert_eq!(engine.original_path(ids[0]).unwrap(), None);
        drop(events);
    }
}
