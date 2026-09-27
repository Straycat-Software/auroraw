// SPDX-License-Identifier: GPL-3.0-or-later
//! The list models: the library grid's (`PhotoGrid`, over the open workspace's photos; the selection is the
//! `GridView`'s and the moves are `gridmath`) and the
//! welcome list's (`KnownWorkspaces`, over the registry). Both are `QAbstractListModel`s, and the base
//! class can only be declared once per link, so they share this bridge.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        #[qobject]
        type QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        #[qproperty(i32, min_rating, cxx_name = "minRating")]
        #[qproperty(i32, selected_count, cxx_name = "selectedCount")]
        #[qproperty(i32, flag_filter, cxx_name = "flagFilter")]
        #[qproperty(QString, keyword_filter, cxx_name = "keywordFilter")]
        #[qproperty(QString, label_filter, cxx_name = "labelFilter")]
        #[qproperty(i32, total)]
        #[qproperty(i32, series_count, cxx_name = "seriesCount")]
        #[qproperty(i32, mark_serial, cxx_name = "markSerial")]
        #[qproperty(i32, selection_series, cxx_name = "selectionSeries")]
        #[qproperty(i32, series_filter, cxx_name = "seriesFilter")]
        type PhotoGrid = super::PhotoGridRust;

        /// Loads the open workspace's photos, newest first, those rated `minRating` or more (the
        /// filter bar's choice; 0 lists them all).
        #[qinvokable]
        fn load(self: Pin<&mut PhotoGrid>);

        /// Lists only the photos rated `min_rating` or more.
        #[qinvokable]
        #[cxx_name = "filterBy"]
        fn filter_by(self: Pin<&mut PhotoGrid>, min_rating: i32);

        /// Lists photos by flag: 0 everything but the rejected (the default), 1 all, 2 picked, 3 rejected.
        /// The rating and keyword filters stay; nothing is selected any more.
        #[qinvokable]
        #[cxx_name = "filterFlags"]
        fn filter_flags(self: Pin<&mut PhotoGrid>, flags: i32);

        /// Lists only the photos with this keyword or one under it (an identifier; empty for no keyword
        /// filter). The other filters stay; nothing is selected any more.
        #[qinvokable]
        #[cxx_name = "filterKeyword"]
        fn filter_keyword(self: Pin<&mut PhotoGrid>, keyword: &QString);

        /// Lists photos by series: 0 all, 1 those in a series, 2 those of an unresolved series, 3 those of a resolved
        /// one. The other filters stay; nothing is selected any more.
        #[qinvokable]
        #[cxx_name = "filterSeries"]
        fn filter_series(self: Pin<&mut PhotoGrid>, kind: i32);

        /// Opens or closes the series of the photo in `row` (a click on its badge, the key `E`); whether it
        /// was a series that could be.
        #[qinvokable]
        #[cxx_name = "toggleSeries"]
        fn toggle_series(self: Pin<&mut PhotoGrid>, row: i32) -> bool;

        /// Opens (or closes) every series.
        #[qinvokable]
        #[cxx_name = "expandAll"]
        fn expand_all(self: Pin<&mut PhotoGrid>, open: bool);

        /// The series of the photo in `row` (an identifier, empty for none) and whether the row is a collapsed
        /// series.
        #[qinvokable]
        #[cxx_name = "seriesAt"]
        fn series_at(self: &PhotoGrid, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "isCollapsed"]
        fn is_collapsed(self: &PhotoGrid, row: i32) -> bool;

        /// The photos selected (a collapsed series counts all its members), identifiers joined by commas, in the
        /// order they are listed; and the listed members of the series a photo is in, in the order they were taken
        /// (empty when it is in none).
        #[qinvokable]
        #[cxx_name = "selectedIds"]
        fn selected_ids(self: &PhotoGrid) -> QString;
        #[qinvokable]
        #[cxx_name = "seriesStateOf"]
        fn series_state_of(self: &PhotoGrid, id: &QString) -> i32;

        #[qinvokable]
        #[cxx_name = "seriesMembersOf"]
        fn series_members_of(self: &PhotoGrid, id: &QString) -> QString;

        /// Marks a photo to keep (a draft, in memory: what `R` keeps when a series is resolved) or takes the mark
        /// off; whether it is marked now. `markSerial` changes with every mark, for what shows them.
        #[qinvokable]
        #[cxx_name = "toggleMark"]
        fn toggle_mark(self: Pin<&mut PhotoGrid>, id: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "isMarked"]
        fn is_marked(self: &PhotoGrid, id: &QString) -> bool;

        /// What was measured on the picture of a photo the image view holds (JSON: sharpness, histogram,
        /// clipped shares), empty until it does.
        #[qinvokable]
        #[cxx_name = "aidsOf"]
        fn aids_of(self: &PhotoGrid, id: &QString) -> QString;

        /// Each photo's sharpness as a percentage of the sharpest of the list (`-1` when not measured yet), for
        /// the identifiers joined by commas, as a JSON array.
        #[qinvokable]
        #[cxx_name = "sharpnessRanks"]
        fn sharpness_ranks(self: &PhotoGrid, ids: &QString) -> QString;

        /// Has these photos' sharpness measured in the background, and their pictures made ahead (identifiers
        /// joined by commas).
        #[qinvokable]
        fn analyse(self: &PhotoGrid, ids: &QString);
        #[qinvokable]
        #[cxx_name = "prefetchIds"]
        fn prefetch_ids(self: &PhotoGrid, ids: &QString);

        /// Groups the selected photos into a new series (one step of the history); how many photos.
        #[qinvokable]
        #[cxx_name = "groupSelection"]
        fn group_selection(self: Pin<&mut PhotoGrid>) -> i32;

        /// Takes the selected photos out of their series, and dissolves the series whose collapsed row is selected.
        #[qinvokable]
        #[cxx_name = "ungroupSelection"]
        fn ungroup_selection(self: Pin<&mut PhotoGrid>) -> i32;

        /// Resolves the series the selection is in, keeping the selected photos (D-035): 1 done, 0 when the
        /// selection is not in exactly one series, -1 when that series is collapsed (open it first).
        #[qinvokable]
        #[cxx_name = "resolveSeries"]
        fn resolve_series(self: Pin<&mut PhotoGrid>) -> i32;

        /// Reopens the resolved series the selection is in; how many.
        #[qinvokable]
        #[cxx_name = "reopenSeries"]
        fn reopen_series(self: Pin<&mut PhotoGrid>) -> i32;

        /// Lists only the photos with this colour label (`red`, `yellow`, `green`, `blue`, `purple`; empty for
        /// any). The other filters stay; nothing is selected any more.
        #[qinvokable]
        #[cxx_name = "filterLabel"]
        fn filter_label(self: Pin<&mut PhotoGrid>, name: &QString);

        /// Flags the selection as one action: `pick`, `reject` (the flag is cleared instead when every
        /// selected photo has it already) or `clear`. How many photos were changed.
        #[qinvokable]
        #[cxx_name = "flagSelection"]
        fn flag_selection(self: Pin<&mut PhotoGrid>, kind: &QString) -> i32;

        /// Gives the selection a colour label as one action: `red`, `yellow`, `green`, `blue` or `purple` (the
        /// label is taken off instead when every selected photo has that colour already), or `none`. How
        /// many photos were changed.
        #[qinvokable]
        #[cxx_name = "labelSelection"]
        fn label_selection(self: Pin<&mut PhotoGrid>, name: &QString) -> i32;

        /// Says which photos the image view will want next when it shows the photo in `row`: the two after it
        /// and the one before, made ahead by the preview service.
        #[qinvokable]
        #[cxx_name = "prefetchAround"]
        fn prefetch_around(self: &PhotoGrid, row: i32);

        /// For each keyword a selected photo carries, how many selected photos carry it, as JSON
        /// (`{"<keyword id>": 3}`): what the keyword panel shows as none, some or all.
        #[qinvokable]
        #[cxx_name = "keywordUsage"]
        fn keyword_usage(self: &PhotoGrid) -> QString;

        /// Adds a keyword (an identifier) to every selected photo, or removes it, as one action. How
        /// many photos.
        #[qinvokable]
        #[cxx_name = "keywordSelection"]
        fn keyword_selection(self: Pin<&mut PhotoGrid>, keyword: &QString, add: bool) -> i32;

        /// Makes the keyword `name` under `parent` (an identifier; empty for the top level) and gives it to
        /// every selected photo, as one action (one step of the history). Its identifier, or `error:` and
        /// why not.
        #[qinvokable]
        #[cxx_name = "createKeywordSelection"]
        fn create_keyword_selection(
            self: Pin<&mut PhotoGrid>,
            name: &QString,
            parent: &QString,
        ) -> QString;

        /// The photo in `row` (its identifier, empty when there is none).
        #[qinvokable]
        #[cxx_name = "idAt"]
        fn id_at(self: &PhotoGrid, row: i32) -> QString;

        /// The photo in `row`'s name and camera, for the image view (which shows its stars, flag and colour
        /// itself).
        #[qinvokable]
        #[cxx_name = "infoAt"]
        fn info_at(self: &PhotoGrid, row: i32) -> QString;

        /// What the grid shows of the photo in `row`: its rating (0 to 5), its flag (0 none, 1 picked, 2
        /// rejected) and its colour label (`red`... or empty). What the image view shows too.
        #[qinvokable]
        #[cxx_name = "ratingAt"]
        fn rating_at(self: &PhotoGrid, row: i32) -> i32;
        #[qinvokable]
        #[cxx_name = "flagAt"]
        fn flag_at(self: &PhotoGrid, row: i32) -> i32;
        #[qinvokable]
        #[cxx_name = "labelAt"]
        fn label_at(self: &PhotoGrid, row: i32) -> QString;

        /// The row of a photo, -1 when it is not listed.
        #[qinvokable]
        #[cxx_name = "rowOf"]
        fn row_of(self: &PhotoGrid, id: &QString) -> i32;

        /// Reads a photo's rating again from the catalogue (the engine says it changed) and redraws
        /// its cell when it is listed.
        #[qinvokable]
        #[cxx_name = "refreshPhoto"]
        fn refresh_photo(self: Pin<&mut PhotoGrid>, id: &QString);

        /// Reads a photo's rating again after an undo or a redo, at once: what the person asked for a
        /// moment ago no longer stands in for what the catalogue says.
        #[qinvokable]
        #[cxx_name = "syncPhoto"]
        fn sync_photo(self: Pin<&mut PhotoGrid>, id: &QString);

        /// Selects only the photo in `row`, and anchors ranges there (a click, an arrow).
        #[qinvokable]
        #[cxx_name = "selectOnly"]
        fn select_only(self: Pin<&mut PhotoGrid>, row: i32);

        /// Adds the photo in `row` to the selection, or removes it (Ctrl+click, Space), and anchors there.
        #[qinvokable]
        fn toggle(self: Pin<&mut PhotoGrid>, row: i32);

        /// Selects the photos from the anchor to `row`: replacing the selection (Shift), or added to it
        /// (`additive`, Ctrl+Shift). The anchor stays.
        #[qinvokable]
        #[cxx_name = "extendTo"]
        fn extend_to(self: Pin<&mut PhotoGrid>, row: i32, additive: bool);

        /// Selects every photo listed.
        #[qinvokable]
        #[cxx_name = "selectAll"]
        fn select_all(self: Pin<&mut PhotoGrid>);

        /// Selects nothing.
        #[qinvokable]
        #[cxx_name = "selectNone"]
        fn select_none(self: Pin<&mut PhotoGrid>);

        /// Selects the photos that are not selected, and only those.
        #[qinvokable]
        fn invert(self: Pin<&mut PhotoGrid>);

        /// Selects exactly these photos (identifiers joined by commas), those that are listed.
        #[qinvokable]
        #[cxx_name = "selectPhotos"]
        fn select_photos(self: Pin<&mut PhotoGrid>, ids: &QString);

        /// Whether the photo in `row` is selected.
        #[qinvokable]
        #[cxx_name = "isSelected"]
        fn is_selected(self: &PhotoGrid, row: i32) -> bool;

        /// The first selected row, -1 when nothing is selected.
        #[qinvokable]
        #[cxx_name = "firstSelectedRow"]
        fn first_selected_row(self: &PhotoGrid) -> i32;

        /// The row ranges start from, -1 when there is none.
        #[qinvokable]
        #[cxx_name = "anchorRow"]
        fn anchor_row(self: &PhotoGrid) -> i32;

        /// A rubber band starts: what is selected now is kept when `additive` (Ctrl).
        #[qinvokable]
        #[cxx_name = "rubberBegin"]
        fn rubber_begin(self: Pin<&mut PhotoGrid>, additive: bool);

        /// The rubber band covers these rows and columns of the grid (`columns` wide): they are selected,
        /// besides what `rubberBegin` kept.
        #[qinvokable]
        #[cxx_name = "rubberTo"]
        fn rubber_to(
            self: Pin<&mut PhotoGrid>,
            first_row: i32,
            last_row: i32,
            first_column: i32,
            last_column: i32,
            columns: i32,
        );

        /// The rubber band is over.
        #[qinvokable]
        #[cxx_name = "rubberEnd"]
        fn rubber_end(self: Pin<&mut PhotoGrid>);

        /// Rates every selected photo (0 clears) as one action, one step of the history; how many were
        /// rated.
        #[qinvokable]
        #[cxx_name = "rateSelection"]
        fn rate_selection(self: Pin<&mut PhotoGrid>, rating: i32) -> i32;

        /// What the status strip says of the photo in `row`: its file, its camera, its stars.
        #[qinvokable]
        #[cxx_name = "summaryAt"]
        fn summary_at(self: &PhotoGrid, row: i32) -> QString;

        /// Where a step of `(dx, dy)` cells from `current` lands, in rows of `columns`.
        #[qinvokable]
        fn step(self: &PhotoGrid, current: i32, dx: i32, dy: i32, columns: i32) -> i32;

        /// Where a keyboard move lands (`page-up`, `page-down`, `home` or `end`), given the selected
        /// index, the columns and the rows that fit on screen.
        #[qinvokable]
        fn jump(self: &PhotoGrid, kind: &QString, current: i32, columns: i32, rows: i32) -> i32;

        /// Rates the photo in `row` (0 clears).
        #[qinvokable]
        #[cxx_name = "setRating"]
        fn set_rating(self: Pin<&mut PhotoGrid>, row: i32, rating: i32);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut PhotoGrid>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut PhotoGrid>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut PhotoGrid>);

        #[inherit]
        fn index(self: &PhotoGrid, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &PhotoGrid, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &PhotoGrid) -> QHash_i32_QByteArray;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &PhotoGrid, _parent: &QModelIndex) -> i32;
    }
    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        type KnownWorkspaces = super::KnownWorkspacesRust;

        /// Reads the registry again.
        #[qinvokable]
        fn refresh(self: Pin<&mut KnownWorkspaces>);

        /// The folder of the workspace in `row`.
        #[qinvokable]
        #[cxx_name = "pathAt"]
        fn path_at(self: &KnownWorkspaces, row: i32) -> QString;

        /// Takes the workspace in `row` off the list (its folder is not touched).
        #[qinvokable]
        fn forget(self: Pin<&mut KnownWorkspaces>, row: i32);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut KnownWorkspaces>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut KnownWorkspaces>);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &KnownWorkspaces, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &KnownWorkspaces) -> QHash_i32_QByteArray;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &KnownWorkspaces, _parent: &QModelIndex) -> i32;
    }

    // Reads the registry once the object exists.
    impl cxx_qt::Initialize for KnownWorkspaces {}

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        type KeywordList = super::KeywordListRust;

        /// Reads the vocabulary and how many photos carry each keyword again.
        #[qinvokable]
        fn refresh(self: Pin<&mut KeywordList>);

        /// Shows the keywords whose name contains `text` (any case) with their ancestors; nothing typed
        /// shows the whole tree, with what was collapsed.
        #[qinvokable]
        #[cxx_name = "setFilter"]
        fn set_filter(self: Pin<&mut KeywordList>, text: &QString);

        /// Collapses or expands the keyword in `row`.
        #[qinvokable]
        #[cxx_name = "toggleExpanded"]
        fn toggle_expanded(self: Pin<&mut KeywordList>, row: i32);

        /// Tells which keywords the selection carries (`KeywordUsage` JSON) and how many photos it has.
        #[qinvokable]
        #[cxx_name = "applyUsage"]
        fn apply_usage(self: Pin<&mut KeywordList>, usage: &QString, selected: i32);

        /// The keyword in `row` (its identifier) and its name.
        #[qinvokable]
        #[cxx_name = "idAt"]
        fn id_at(self: &KeywordList, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "nameAt"]
        fn name_at(self: &KeywordList, row: i32) -> QString;

        /// The row of the best match for what was typed (the name itself, else the first that starts with
        /// it, else the first that contains it), -1 when there is none.
        #[qinvokable]
        #[cxx_name = "bestMatch"]
        fn best_match(self: &KeywordList, text: &QString) -> i32;

        /// Adds `name` to the vocabulary under the keyword `parent` (an identifier; empty for the top
        /// level); its identifier, or the existing keyword's when that name is there already, or `error:`
        /// and why not.
        #[qinvokable]
        fn create(self: Pin<&mut KeywordList>, name: &QString, parent: &QString) -> QString;

        /// Renames the keyword in `row`; empty, or why not.
        #[qinvokable]
        fn rename(self: Pin<&mut KeywordList>, row: i32, name: &QString) -> QString;

        /// The identifier of the keyword named `name` (any case) under `parent` (empty for the top level),
        /// or empty when there is none.
        #[qinvokable]
        #[cxx_name = "findSibling"]
        fn find_sibling(self: &KeywordList, name: &QString, parent: &QString) -> QString;

        /// The name of the keyword `id`, or empty when it is not in the vocabulary.
        #[qinvokable]
        #[cxx_name = "nameOf"]
        fn name_of(self: &KeywordList, id: &QString) -> QString;

        /// Whether the keyword `id` is still in the vocabulary.
        #[qinvokable]
        #[cxx_name = "hasKeyword"]
        fn has_keyword(self: &KeywordList, id: &QString) -> bool;

        /// Whether the keyword `id` can be put under `parent` (empty for the top level).
        #[qinvokable]
        #[cxx_name = "canMove"]
        fn can_move(self: &KeywordList, id: &QString, parent: &QString) -> bool;

        /// Where `id` can go (JSON `[{"id", "path"}]`), for the Move dialog.
        #[qinvokable]
        #[cxx_name = "moveTargets"]
        fn move_targets(self: &KeywordList, id: &QString) -> QString;

        /// What deleting `id` takes with it (JSON `{"name", "keywords", "photos"}`).
        #[qinvokable]
        fn branch(self: &KeywordList, id: &QString) -> QString;

        /// Puts `id` under `parent` (empty for the top level); empty, or why not.
        #[qinvokable]
        #[cxx_name = "moveKeyword"]
        fn move_keyword(self: Pin<&mut KeywordList>, id: &QString, parent: &QString) -> QString;

        /// Deletes `id` and its branch; empty, or why not.
        #[qinvokable]
        fn remove(self: Pin<&mut KeywordList>, id: &QString) -> QString;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut KeywordList>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut KeywordList>);
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut KeywordList>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
        #[inherit]
        fn index(self: &KeywordList, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &KeywordList, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &KeywordList) -> QHash_i32_QByteArray;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &KeywordList, _parent: &QModelIndex) -> i32;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        #[qproperty(QString, job)]
        type SourceList = super::SourceListRust;

        /// Reads the open workspace's sources again.
        #[qinvokable]
        fn refresh(self: Pin<&mut SourceList>);

        /// The folder `text` names, as the application understands it (canonical); `error:` and
        /// why not.
        #[qinvokable]
        fn resolve(self: &SourceList, text: &QString) -> QString;

        /// What adding `folder` would do: `free`, `inside:<the source>`, or `contains:<the sources,
        /// quoted and separated by commas>`; `error:` and why not.
        #[qinvokable]
        fn plan(self: &SourceList, folder: &QString) -> QString;

        /// Adds `folder` as a source and starts its scan (`job` is its job); empty, or why not.
        /// `merge` takes the sources inside it into the new one.
        #[qinvokable]
        fn add(
            self: Pin<&mut SourceList>,
            folder: &QString,
            name: &QString,
            merge: bool,
        ) -> QString;

        /// How many photos the source in `row` has, and how many of them have work in them (read
        /// from the catalogue now, not from the list).
        #[qinvokable]
        #[cxx_name = "photosAt"]
        fn photos_at(self: &SourceList, row: i32) -> i32;
        #[qinvokable]
        #[cxx_name = "workedOnAt"]
        fn worked_on_at(self: &SourceList, row: i32) -> i32;

        /// Takes the source in `row` out of the catalogue, in the background; empty, or why not.
        #[qinvokable]
        fn remove(self: Pin<&mut SourceList>, row: i32) -> QString;

        /// Scans the source in `row` again; empty, or why not.
        #[qinvokable]
        fn rescan(self: Pin<&mut SourceList>, row: i32) -> QString;

        /// Scans the source whose folder is `folder` (an import made it a source); empty, or why not.
        #[qinvokable]
        #[cxx_name = "rescanFolder"]
        fn rescan_folder(self: Pin<&mut SourceList>, folder: &QString) -> QString;

        /// Answers the scan's question about photos removed earlier.
        #[qinvokable]
        #[cxx_name = "continueScan"]
        fn continue_scan(self: &SourceList, restore: bool);

        /// Stops the scan that waits at its question.
        #[qinvokable]
        #[cxx_name = "cancelScan"]
        fn cancel_scan(self: &SourceList);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut SourceList>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut SourceList>);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &SourceList, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &SourceList) -> QHash_i32_QByteArray;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &SourceList, _parent: &QModelIndex) -> i32;
    }
}

use core::pin::Pin;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant};

use auroraw_catalogue::{Cursor, Filter, FlagFilter};
use auroraw_engine::{Command, Engine, KnownWorkspace};
use auroraw_types::{PhotoId, SeriesId};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};

use crate::grid_items::{self, Item};
use crate::keyword_list::KeywordListRust;
use crate::selection::Selection;
use crate::session;
use crate::source_list::SourceListRust;

/// Qt::UserRole and the next one.
const ROLE_PHOTO_ID: i32 = 0x0100;
const ROLE_RATING: i32 = 0x0101;
const ROLE_SELECTED: i32 = 0x0102;
const ROLE_FLAG: i32 = 0x0103;
const ROLE_LABEL: i32 = 0x0104;
const ROLE_SERIES_ID: i32 = 0x0105;
const ROLE_SERIES_SIZE: i32 = 0x0106;
const ROLE_SERIES_TOTAL: i32 = 0x0107;
const ROLE_SERIES_RESOLVED: i32 = 0x0108;
const ROLE_SERIES_OPEN: i32 = 0x0109;
const ROLE_SERIES_EDGE: i32 = 0x010A;
const ROLE_MARKED: i32 = 0x010B;

/// A label's code: 0 for none, or a label that is not one of the five (another program's).
fn label_code(label: Option<&str>) -> u8 {
    use auroraw_engine::ColourLabel;
    label
        .and_then(|text| text.parse::<ColourLabel>().ok())
        .and_then(|colour| ColourLabel::ALL.iter().position(|c| *c == colour))
        .map_or(0, |i| i as u8 + 1)
}

/// The name QML knows a label by (`red`, `yellow`...), empty for none.
fn label_name(code: u8) -> &'static str {
    ["", "red", "yellow", "green", "blue", "purple"]
        .get(usize::from(code))
        .copied()
        .unwrap_or("")
}

/// What this grid asked the engine for and has not seen it confirm: until the catalogue says the same (or
/// a moment passes), what it says is older than what was asked, and not to be shown.
struct Pending {
    rating: Option<u8>,
    flag: Option<u8>,
    label: Option<u8>,
    at: Instant,
}

/// The Rust side of the grid model.
#[derive(Default)]
pub struct PhotoGridRust {
    count: i32,
    min_rating: i32,
    flag_filter: i32,
    keyword_filter: QString,
    label_filter: QString,
    /// Everything the filters list, in order (the rows are derived from it: a collapsed series is one of them).
    all: Vec<Item>,
    /// Where each photo of `all` is, so that a change to one photo reaches the copy a collapsed series hides.
    all_pos: HashMap<PhotoId, usize>,
    series_info: HashMap<SeriesId, auroraw_catalogue::SeriesInfo>,
    /// The series shown expanded.
    open: std::collections::HashSet<SeriesId>,
    /// The rows: photos, and collapsed series that stand for their members.
    items: Vec<Item>,
    /// Where each listed photo is.
    rows: HashMap<PhotoId, usize>,
    /// A photo that a collapsed series' row hides, and the row's photo.
    hidden: HashMap<PhotoId, PhotoId>,
    series_filter: i32,
    mark_serial: i32,
    /// The photos marked to keep (drafts, in memory: D-103).
    marks: std::collections::HashSet<PhotoId>,
    total: i32,
    series_count: i32,
    /// The ratings and flags asked for and not yet confirmed (a quick series of keys must not flicker back).
    pending: HashMap<PhotoId, Pending>,
    selected_count: i32,
    /// What the selection holds as to series: 1 a photo in none, 2 a photo of an unresolved series, 4 a photo of a
    /// resolved one (the commands that a resolved series does not take are off for it).
    selection_series: i32,
    /// What was selected when a rubber band started that adds to it.
    rubber_base: Option<std::collections::HashSet<PhotoId>>,
    /// The photos selected, by identifier, and where ranges start (D-097).
    selection: Selection,
}

/// How long an unconfirmed rating is trusted over the catalogue (a command the engine refused).
const PENDING_FOR: Duration = Duration::from_secs(2);

/// Every photo the catalogue lists in the grid's own order (spike 3: the whole ordered list is cheap
/// even at 100,000 photos; only thumbnails are lazy), rated `min_rating` or more.
fn load_items(filter: &Filter) -> (Vec<Item>, HashMap<SeriesId, auroraw_catalogue::SeriesInfo>) {
    let Some(session) = session::current() else {
        return (Vec::new(), HashMap::new());
    };
    let Ok(catalogue) = session.engine.read_catalogue() else {
        return (Vec::new(), HashMap::new());
    };
    let mut items = Vec::new();
    let mut after = None;
    loop {
        let Ok(page) = catalogue.list_filtered(filter, after, 5000) else {
            break;
        };
        if page.is_empty() {
            break;
        }
        after = page.last().map(|row| Cursor {
            capture_time: row.capture_time,
            id: row.id,
        });
        items.extend(page.into_iter().map(|row| {
            Item::photo(
                row.id,
                row.effective_rating,
                row.effective_flag,
                label_code(row.label.as_deref()),
                row.series_id,
            )
        }));
    }
    let info = catalogue
        .series_infos()
        .unwrap_or_default()
        .into_iter()
        .map(|s| (s.id, s))
        .collect();
    (items, info)
}

impl PhotoGridRust {
    /// Opening a collapsed series selects its members when its row was selected (it stood for them).
    fn open_series(&mut self, series: SeriesId) {
        if !self.open.insert(series) {
            return;
        }
        let rep = self
            .items
            .iter()
            .find(|i| i.series == Some(series) && !i.members.is_empty())
            .cloned();
        if let Some(rep) = rep
            && self.selection.contains(&rep.id)
        {
            let mut selected = self.selection.snapshot();
            selected.remove(&rep.id);
            selected.extend(rep.members.iter().copied());
            self.selection.set(selected.into_iter().collect::<Vec<_>>());
        }
    }
}

impl qobject::PhotoGrid {
    pub fn load(mut self: Pin<&mut Self>) {
        let (mut all, info) = load_items(&self.filter());
        // A rating or a flag asked for a moment ago may not be in the catalogue yet: it stays what was asked.
        self.as_mut()
            .rust_mut()
            .pending
            .retain(|_, pending| pending.at.elapsed() < PENDING_FOR);
        for item in &mut all {
            if let Some(pending) = self.pending.get(&item.id) {
                item.rating = pending.rating.unwrap_or(item.rating);
                item.flag = pending.flag.unwrap_or(item.flag);
                item.label = pending.label.unwrap_or(item.label);
            }
        }
        let positions = all
            .iter()
            .enumerate()
            .map(|(i, item)| (item.id, i))
            .collect();
        self.as_mut().rust_mut().all = all;
        self.as_mut().rust_mut().all_pos = positions;
        self.as_mut().rust_mut().series_info = info;
        self.rebuild();
    }

    /// Writes what is known of a photo's rating, flag or colour into the list of everything listed, so that a row
    /// made later (a series that opens) shows it, as a photo a collapsed series hides has no row now.
    fn note_in_all(
        mut self: Pin<&mut Self>,
        id: PhotoId,
        rating: Option<u8>,
        flag: Option<u8>,
        label: Option<u8>,
    ) {
        let Some(&at) = self.all_pos.get(&id) else {
            return;
        };
        let item = &mut self.as_mut().rust_mut().all[at];
        item.rating = rating.unwrap_or(item.rating);
        item.flag = flag.unwrap_or(item.flag);
        item.label = label.unwrap_or(item.label);
    }

    /// Makes the rows from what the filters list and which series are open, and tells the views.
    fn rebuild(mut self: Pin<&mut Self>) {
        let (items, hidden) = grid_items::view(&self.all, &self.series_info, &self.open);
        let count = items.len() as i32;
        let total = self.all.len() as i32;
        let rows = items
            .iter()
            .enumerate()
            .map(|(row, item)| (item.id, row))
            .collect();
        // SAFETY: every begin is followed by its end, with nothing in between that can fail.
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().items = items;
            self.as_mut().rust_mut().rows = rows;
            self.as_mut().rust_mut().hidden = hidden;
            self.as_mut().end_reset_model();
        }
        self.as_mut().set_count(count);
        self.as_mut().set_total(total);
        let series = self.series_info.len() as i32;
        self.as_mut().set_series_count(series);
        // What was selected and is still listed stays selected; a photo a collapsed series hides is its row.
        let mapped: Vec<PhotoId> = self
            .selection
            .snapshot()
            .into_iter()
            .map(|id| self.hidden.get(&id).copied().unwrap_or(id))
            .collect();
        let listed = self.ids();
        self.as_mut().rust_mut().selection.set(mapped);
        self.as_mut().rust_mut().selection.retain(&listed);
        self.sync_selection_state();
    }

    /// How many photos are selected (a collapsed series' row counts its members).
    /// Tells the views how many photos are selected and what they are as to series.
    fn sync_selection_state(mut self: Pin<&mut Self>) {
        let selected = self.selected_photo_count();
        let mut state = 0;
        for item in self.items.iter().filter(|i| self.selection.contains(&i.id)) {
            state |= match item.series {
                None => 1,
                Some(_) if item.series_resolved => 4,
                Some(_) => 2,
            };
        }
        self.as_mut().set_selected_count(selected);
        self.as_mut().set_selection_series(state);
    }

    fn selected_photo_count(&self) -> i32 {
        self.items
            .iter()
            .filter(|item| self.selection.contains(&item.id))
            .map(|item| item.members.len().max(1) as i32)
            .sum()
    }

    /// The photos the rows stand for, each once.
    fn photos_of(&self, rows: &[usize]) -> Vec<PhotoId> {
        let mut out: Vec<PhotoId> = Vec::new();
        for row in rows {
            for id in self.items[*row].photos() {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        out
    }

    pub fn filter_by(mut self: Pin<&mut Self>, min_rating: i32) {
        self.as_mut().set_min_rating(min_rating.clamp(0, 5));
        // A new filter is a new list: nothing of the old one stays selected.
        self.as_mut().rust_mut().selection.none();
        self.load();
    }

    /// What the grid lists now: the rating, the flags and the keyword filters together.
    fn filter(&self) -> Filter {
        Filter {
            min_rating: self.min_rating.clamp(0, 5) as u8,
            flags: match self.flag_filter {
                1 => FlagFilter::All,
                2 => FlagFilter::Picked,
                3 => FlagFilter::Rejected,
                _ => FlagFilter::NotRejected,
            },
            keyword: auroraw_types::KeywordId::from_str(&self.keyword_filter.to_string()).ok(),
            series: match self.series_filter {
                1 => auroraw_catalogue::SeriesFilter::InSeries,
                2 => auroraw_catalogue::SeriesFilter::Unresolved,
                3 => auroraw_catalogue::SeriesFilter::Resolved,
                _ => auroraw_catalogue::SeriesFilter::Any,
            },
            label: self
                .label_filter
                .to_string()
                .parse::<auroraw_engine::ColourLabel>()
                .ok()
                .map(|colour| colour.name().to_string()),
        }
    }

    pub fn filter_flags(mut self: Pin<&mut Self>, flags: i32) {
        self.as_mut().set_flag_filter(flags.clamp(0, 3));
        self.as_mut().rust_mut().selection.none();
        self.load();
    }

    pub fn filter_keyword(mut self: Pin<&mut Self>, keyword: &QString) {
        self.as_mut().set_keyword_filter(keyword.clone());
        self.as_mut().rust_mut().selection.none();
        self.load();
    }

    pub fn filter_series(mut self: Pin<&mut Self>, kind: i32) {
        self.as_mut().set_series_filter(kind.clamp(0, 3));
        self.as_mut().rust_mut().selection.none();
        self.load();
    }

    /// The listed members of a series that could be shown open: at least two.
    fn openable(&self, series: &SeriesId) -> bool {
        self.all
            .iter()
            .filter(|i| i.series.as_ref() == Some(series))
            .count()
            >= 2
    }

    pub fn toggle_series(mut self: Pin<&mut Self>, row: i32) -> bool {
        let Some(series) = usize::try_from(row)
            .ok()
            .and_then(|row| self.items.get(row))
            .and_then(|item| item.series)
        else {
            return false;
        };
        if !self.openable(&series) {
            return false;
        }
        if self.open.contains(&series) {
            self.as_mut().rust_mut().open.remove(&series);
        } else {
            self.as_mut().rust_mut().open_series(series);
        }
        self.rebuild();
        true
    }

    pub fn expand_all(mut self: Pin<&mut Self>, open: bool) {
        if open {
            let all: Vec<SeriesId> = self.series_info.keys().copied().collect();
            for series in all {
                if self.openable(&series) {
                    self.as_mut().rust_mut().open_series(series);
                }
            }
        } else {
            self.as_mut().rust_mut().open.clear();
        }
        self.rebuild();
    }

    /// What a photo is as to series, as `selectionSeries` says of the selection: 1 in none, 2 in an unresolved
    /// series, 4 in a resolved one, 0 for a photo that is not listed.
    pub fn series_state_of(&self, id: &QString) -> i32 {
        PhotoId::from_str(&id.to_string())
            .ok()
            .and_then(|id| {
                let id = self.hidden.get(&id).copied().unwrap_or(id);
                self.rows.get(&id).map(|row| &self.items[*row])
            })
            .map_or(0, |item| match item.series {
                None => 1,
                Some(_) if item.series_resolved => 4,
                Some(_) => 2,
            })
    }

    pub fn series_at(&self, row: i32) -> QString {
        self.item_at(row)
            .and_then(|item| item.series)
            .map(|s| QString::from(s.to_string().as_str()))
            .unwrap_or_default()
    }

    pub fn is_collapsed(&self, row: i32) -> bool {
        self.item_at(row)
            .is_some_and(|item| !item.members.is_empty())
    }

    pub fn group_selection(self: Pin<&mut Self>) -> i32 {
        let Some(session) = session::current() else {
            return 0;
        };
        let photos = self.photos_of(&self.selected_rows());
        if photos.len() < 2 {
            return 0;
        }
        let count = photos.len() as i32;
        let _ = session.engine.submit(Command::GroupPhotos { photos });
        count
    }

    pub fn ungroup_selection(self: Pin<&mut Self>) -> i32 {
        let Some(session) = session::current() else {
            return 0;
        };
        let mut dissolve: Vec<SeriesId> = Vec::new();
        let mut leave: Vec<PhotoId> = Vec::new();
        for row in self.selected_rows() {
            let item = &self.items[row];
            let Some(series) = item.series else {
                continue;
            };
            if item.members.is_empty() {
                leave.push(item.id);
            } else if !dissolve.contains(&series) {
                dissolve.push(series);
            }
        }
        let count = (leave.len() + dissolve.len()) as i32;
        if !leave.is_empty() {
            let _ = session
                .engine
                .submit(Command::RemoveFromSeries { photos: leave });
        }
        for series in dissolve {
            let _ = session.engine.submit(Command::DissolveSeries { series });
        }
        count
    }

    /// The series a photo is in, from everything the filters list.
    fn series_of_photo(&self, photo: &PhotoId) -> Option<SeriesId> {
        self.all
            .iter()
            .find(|i| i.id == *photo)
            .and_then(|i| i.series)
    }

    /// The photos of a series that are marked to keep, in the order they are listed.
    fn marked_in(&self, series: &SeriesId) -> Vec<PhotoId> {
        self.all
            .iter()
            .filter(|i| i.series.as_ref() == Some(series) && self.marks.contains(&i.id))
            .map(|i| i.id)
            .collect()
    }

    pub fn resolve_series(mut self: Pin<&mut Self>) -> i32 {
        let Some(session) = session::current() else {
            return 0;
        };
        let mut series: Option<SeriesId> = None;
        let mut selected: Vec<PhotoId> = Vec::new();
        let mut collapsed = false;
        for row in self.selected_rows() {
            let item = &self.items[row];
            let Some(s) = item.series else {
                continue;
            };
            if series.is_some_and(|other| other != s) {
                return 0;
            }
            series = Some(s);
            collapsed |= !item.members.is_empty();
            selected.push(item.id);
        }
        // With nothing selected in a series, the marks say which series it is (when they are all in one).
        if series.is_none() {
            let mut of_marks: Vec<SeriesId> = Vec::new();
            for photo in self.marks.iter() {
                if let Some(s) = self.series_of_photo(photo)
                    && !of_marks.contains(&s)
                {
                    of_marks.push(s);
                }
            }
            if of_marks.len() == 1 {
                series = of_marks.pop();
            }
        }
        let Some(series) = series else {
            return 0;
        };
        // The marks are what to keep; without any, the selection (D-101).
        let marked = self.marked_in(&series);
        let keep = if marked.is_empty() {
            if collapsed {
                return -1;
            }
            selected
        } else {
            marked
        };
        let _ = session
            .engine
            .submit(Command::ResolveSeries { series, keep });
        self.as_mut().clear_marks_of(&series);
        1
    }

    /// Takes the marks off every photo of a series.
    fn clear_marks_of(mut self: Pin<&mut Self>, series: &SeriesId) {
        let members: Vec<PhotoId> = self
            .all
            .iter()
            .filter(|i| i.series.as_ref() == Some(series))
            .map(|i| i.id)
            .collect();
        let before = self.marks.len();
        for member in &members {
            self.as_mut().rust_mut().marks.remove(member);
        }
        if self.marks.len() != before {
            self.marks_changed();
        }
    }

    /// Tells the views the marks changed (every row's `marked`, and the serial that what shows them follows).
    fn marks_changed(mut self: Pin<&mut Self>) {
        let serial = *self.mark_serial() + 1;
        self.as_mut().set_mark_serial(serial);
        let last = self.items.len() as i32 - 1;
        if last >= 0 {
            let (first, end) = (
                self.index(0, 0, &QModelIndex::default()),
                self.index(last, 0, &QModelIndex::default()),
            );
            let mut roles = QVector::<i32>::default();
            roles.append(ROLE_MARKED);
            self.as_mut().data_changed(&first, &end, &roles);
        }
    }

    pub fn toggle_mark(mut self: Pin<&mut Self>, id: &QString) -> bool {
        let Ok(photo) = PhotoId::from_str(&id.to_string()) else {
            return false;
        };
        let marked = if self.marks.contains(&photo) {
            self.as_mut().rust_mut().marks.remove(&photo);
            false
        } else {
            self.as_mut().rust_mut().marks.insert(photo);
            true
        };
        self.marks_changed();
        marked
    }

    pub fn selected_ids(&self) -> QString {
        let ids: Vec<String> = self
            .photos_of(&self.selected_rows())
            .iter()
            .map(ToString::to_string)
            .collect();
        QString::from(ids.join(",").as_str())
    }

    pub fn series_members_of(&self, id: &QString) -> QString {
        let Some(series) = PhotoId::from_str(&id.to_string())
            .ok()
            .and_then(|photo| self.series_of_photo(&photo))
        else {
            return QString::default();
        };
        // `all` lists the newest first: the frames in the order they were taken are the reverse.
        let ids: Vec<String> = self
            .all
            .iter()
            .rev()
            .filter(|i| i.series.as_ref() == Some(&series))
            .map(|i| i.id.to_string())
            .collect();
        QString::from(ids.join(",").as_str())
    }

    pub fn is_marked(&self, id: &QString) -> bool {
        PhotoId::from_str(&id.to_string()).is_ok_and(|photo| self.marks.contains(&photo))
    }

    fn ids_from(text: &QString) -> Vec<PhotoId> {
        text.to_string()
            .split(',')
            .filter_map(|t| PhotoId::from_str(t).ok())
            .collect()
    }

    pub fn aids_of(&self, id: &QString) -> QString {
        let Some(session) = session::current() else {
            return QString::default();
        };
        let Some(aids) = PhotoId::from_str(&id.to_string())
            .ok()
            .and_then(|photo| session.previews.service().aids(&photo))
        else {
            return QString::default();
        };
        QString::from(
            serde_json::json!({
                "sharpness": aids.sharpness,
                "histogram": aids.histogram.iter().map(|c| c.to_vec()).collect::<Vec<_>>(),
                "high": aids.clipped_high,
                "low": aids.clipped_low,
            })
            .to_string()
            .as_str(),
        )
    }

    pub fn sharpness_ranks(&self, ids: &QString) -> QString {
        let Some(session) = session::current() else {
            return QString::from("[]");
        };
        let service = session.previews.service();
        let scores: Vec<Option<f32>> = Self::ids_from(ids)
            .iter()
            .map(|id| service.sharpness(id))
            .collect();
        let best = scores.iter().flatten().copied().fold(0.0f32, f32::max);
        let ranks: Vec<i32> = scores
            .iter()
            .map(|score| match score {
                Some(score) if best > 0.0 => (score / best * 100.0).round() as i32,
                Some(_) => 100,
                None => -1,
            })
            .collect();
        QString::from(serde_json::json!(ranks).to_string().as_str())
    }

    pub fn analyse(&self, ids: &QString) {
        if let Some(session) = session::current() {
            session.previews.service().analyse(&Self::ids_from(ids));
        }
    }

    pub fn prefetch_ids(&self, ids: &QString) {
        if let Some(session) = session::current() {
            session.previews.prefetch(&Self::ids_from(ids));
        }
    }

    pub fn reopen_series(self: Pin<&mut Self>) -> i32 {
        let Some(session) = session::current() else {
            return 0;
        };
        let mut series: Vec<SeriesId> = Vec::new();
        for row in self.selected_rows() {
            let item = &self.items[row];
            if let Some(s) = item.series
                && item.series_resolved
                && !series.contains(&s)
            {
                series.push(s);
            }
        }
        let count = series.len() as i32;
        for series in series {
            let _ = session.engine.submit(Command::ReopenSeries { series });
        }
        count
    }

    pub fn filter_label(mut self: Pin<&mut Self>, name: &QString) {
        self.as_mut().set_label_filter(name.clone());
        self.as_mut().rust_mut().selection.none();
        self.load();
    }

    fn ids(&self) -> Vec<PhotoId> {
        self.items.iter().map(|item| item.id).collect()
    }

    fn id_of(&self, row: i32) -> Option<PhotoId> {
        usize::try_from(row)
            .ok()
            .and_then(|row| self.items.get(row))
            .map(|item| item.id)
    }

    /// Tells the views that the selection changed: every row's `selected` (only the ones on screen cost).
    fn selection_changed(mut self: Pin<&mut Self>) {
        let last = self.items.len() as i32 - 1;
        if last >= 0 {
            let (first, end) = (
                self.index(0, 0, &QModelIndex::default()),
                self.index(last, 0, &QModelIndex::default()),
            );
            let mut roles = QVector::<i32>::default();
            roles.append(ROLE_SELECTED);
            self.as_mut().data_changed(&first, &end, &roles);
        }
        self.sync_selection_state();
    }

    pub fn select_only(mut self: Pin<&mut Self>, row: i32) {
        if let Some(id) = self.id_of(row) {
            self.as_mut().rust_mut().selection.only(id);
            self.selection_changed();
        }
    }

    pub fn toggle(mut self: Pin<&mut Self>, row: i32) {
        if let Some(id) = self.id_of(row) {
            self.as_mut().rust_mut().selection.toggle(id);
            self.selection_changed();
        }
    }

    pub fn extend_to(mut self: Pin<&mut Self>, row: i32, additive: bool) {
        let Some(target) = usize::try_from(row).ok().filter(|r| *r < self.items.len()) else {
            return;
        };
        let anchor = self
            .selection
            .anchor()
            .and_then(|id| self.rows.get(&id).copied())
            .unwrap_or(target);
        let ids = self.ids();
        self.as_mut()
            .rust_mut()
            .selection
            .range(&ids, anchor, target, additive);
        self.selection_changed();
    }

    pub fn select_all(mut self: Pin<&mut Self>) {
        let ids = self.ids();
        self.as_mut().rust_mut().selection.all(&ids);
        self.selection_changed();
    }

    pub fn select_none(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().selection.none();
        self.selection_changed();
    }

    pub fn invert(mut self: Pin<&mut Self>) {
        let ids = self.ids();
        self.as_mut().rust_mut().selection.invert(&ids);
        self.selection_changed();
    }

    pub fn select_photos(mut self: Pin<&mut Self>, ids: &QString) {
        let listed: Vec<PhotoId> = ids
            .to_string()
            .split(',')
            .filter_map(|text| PhotoId::from_str(text).ok())
            // A photo that a collapsed series hides is selected through its row.
            .map(|id| self.hidden.get(&id).copied().unwrap_or(id))
            .filter(|id| self.rows.contains_key(id))
            .collect();
        self.as_mut().rust_mut().selection.set(listed);
        self.selection_changed();
    }

    pub fn is_selected(&self, row: i32) -> bool {
        self.id_of(row)
            .is_some_and(|id| self.selection.contains(&id))
    }

    pub fn first_selected_row(&self) -> i32 {
        self.items
            .iter()
            .position(|item| self.selection.contains(&item.id))
            .map_or(-1, |row| row as i32)
    }

    pub fn anchor_row(&self) -> i32 {
        self.selection
            .anchor()
            .and_then(|id| self.rows.get(&id).copied())
            .map_or(-1, |row| row as i32)
    }

    /// The selected photos in row order.
    fn selected_rows(&self) -> Vec<usize> {
        (0..self.items.len())
            .filter(|row| self.selection.contains(&self.items[*row].id))
            .collect()
    }

    pub fn flag_selection(mut self: Pin<&mut Self>, kind: &QString) -> i32 {
        use auroraw_engine::Flag;
        let Some(session) = session::current() else {
            return 0;
        };
        let target: u8 = match kind.to_string().as_str() {
            "pick" => 1,
            "reject" => 2,
            _ => 0,
        };
        let rows = self.selected_rows();
        if rows.is_empty() {
            return 0;
        }
        // The same key again takes the flag off: when every selected photo has it already.
        let all_have = rows.iter().all(|row| self.items[*row].flag == target);
        let new = if target != 0 && all_have { 0 } else { target };
        let flag = match new {
            1 => Some(Flag::Picked),
            2 => Some(Flag::Rejected),
            _ => None,
        };
        let mut commands: Vec<Command> = self
            .photos_of(&rows)
            .into_iter()
            .map(|photo_id| Command::SetFlag { photo_id, flag })
            .collect();
        let command = if commands.len() == 1 {
            commands.remove(0)
        } else {
            Command::Batch { commands }
        };
        let _ = session.engine.submit(command);
        // The cells show it at once (a rejected one stays, dimmed, until the list is read again).
        for row in &rows {
            self.as_mut().rust_mut().items[*row].flag = new;
            // (A collapsed series' row stands for all its members.)
            for id in self.items[*row].photos() {
                self.as_mut().note_in_all(id, None, Some(new), None);
                self.as_mut().ask(id, None, Some(new), None);
            }
        }
        self.as_mut().redraw_all();
        rows.len() as i32
    }

    fn item_at(&self, row: i32) -> Option<&Item> {
        usize::try_from(row)
            .ok()
            .and_then(|row| self.items.get(row))
    }

    pub fn rating_at(&self, row: i32) -> i32 {
        self.item_at(row).map_or(0, |i| i32::from(i.rating))
    }

    pub fn flag_at(&self, row: i32) -> i32 {
        self.item_at(row).map_or(0, |i| i32::from(i.flag))
    }

    pub fn label_at(&self, row: i32) -> QString {
        QString::from(self.item_at(row).map_or("", |i| label_name(i.label)))
    }

    pub fn label_selection(mut self: Pin<&mut Self>, name: &QString) -> i32 {
        use auroraw_engine::ColourLabel;
        let Some(session) = session::current() else {
            return 0;
        };
        let target: u8 = match name.to_string().as_str() {
            "red" => 1,
            "yellow" => 2,
            "green" => 3,
            "blue" => 4,
            "purple" => 5,
            _ => 0,
        };
        let rows = self.selected_rows();
        if rows.is_empty() {
            return 0;
        }
        // The same colour again takes it off: when every selected photo has it already.
        let all_have = rows.iter().all(|row| self.items[*row].label == target);
        let new = if target != 0 && all_have { 0 } else { target };
        let label = (new != 0).then(|| ColourLabel::ALL[usize::from(new) - 1]);
        let mut commands: Vec<Command> = self
            .photos_of(&rows)
            .into_iter()
            .map(|photo_id| Command::SetLabel { photo_id, label })
            .collect();
        let command = if commands.len() == 1 {
            commands.remove(0)
        } else {
            Command::Batch { commands }
        };
        let _ = session.engine.submit(command);
        for row in &rows {
            self.as_mut().rust_mut().items[*row].label = new;
            // (A collapsed series' row stands for all its members.)
            for id in self.items[*row].photos() {
                self.as_mut().note_in_all(id, None, None, Some(new));
                self.as_mut().ask(id, None, None, Some(new));
            }
        }
        self.as_mut().redraw_all();
        rows.len() as i32
    }

    pub fn prefetch_around(&self, row: i32) {
        let Some(session) = session::current() else {
            return;
        };
        let Ok(row) = usize::try_from(row) else {
            return;
        };
        let mut next: Vec<PhotoId> = Vec::new();
        for r in [row + 1, row + 2] {
            if let Some(item) = self.items.get(r) {
                next.push(item.id);
            }
        }
        if let Some(item) = row.checked_sub(1).and_then(|r| self.items.get(r)) {
            next.push(item.id);
        }
        session.previews.prefetch(&next);
    }

    pub fn keyword_usage(&self) -> QString {
        let Some(session) = session::current() else {
            return QString::from("{}");
        };
        let selected: Vec<PhotoId> = self.photos_of(&self.selected_rows());
        let usage = session
            .engine
            .read_catalogue()
            .ok()
            .and_then(|catalogue| catalogue.keyword_usage(&selected).ok())
            .unwrap_or_default();
        let map: serde_json::Map<String, serde_json::Value> = usage
            .into_iter()
            .map(|(id, count)| (id.to_string(), serde_json::Value::from(count)))
            .collect();
        QString::from(serde_json::Value::Object(map).to_string().as_str())
    }

    pub fn keyword_selection(self: Pin<&mut Self>, keyword: &QString, add: bool) -> i32 {
        let Some(session) = session::current() else {
            return 0;
        };
        let Ok(keyword_id) = auroraw_types::KeywordId::from_str(&keyword.to_string()) else {
            return 0;
        };
        let rows = self.selected_rows();
        if rows.is_empty() {
            return 0;
        }
        let mut commands: Vec<Command> = self
            .photos_of(&rows)
            .into_iter()
            .map(|photo_id| {
                if add {
                    Command::AddKeyword {
                        photo_id,
                        keyword_id,
                    }
                } else {
                    Command::RemoveKeyword {
                        photo_id,
                        keyword_id,
                    }
                }
            })
            .collect();
        let command = if commands.len() == 1 {
            commands.remove(0)
        } else {
            Command::Batch { commands }
        };
        let _ = session.engine.submit(command);
        rows.len() as i32
    }

    pub fn create_keyword_selection(
        self: Pin<&mut Self>,
        name: &QString,
        parent: &QString,
    ) -> QString {
        let Some(session) = session::current() else {
            return QString::from("error:other:No workspace is open.");
        };
        let rows = self.selected_rows();
        let keyword_id = auroraw_types::KeywordId::random();
        let parent = auroraw_types::KeywordId::from_str(&parent.to_string()).ok();
        let mut commands = vec![Command::CreateKeyword {
            name: name.to_string(),
            parent,
            id: Some(keyword_id),
        }];
        commands.extend(
            self.photos_of(&rows)
                .into_iter()
                .map(|photo_id| Command::AddKeyword {
                    photo_id,
                    keyword_id,
                }),
        );
        let command = if commands.len() == 1 {
            commands.remove(0)
        } else {
            Command::Batch { commands }
        };
        match session.engine.submit_and_wait(command) {
            Ok(_) => QString::from(keyword_id.to_string().as_str()),
            Err(e) => QString::from(format!("error:{}", crate::keyword_list::reason(&e)).as_str()),
        }
    }

    /// Redraws every cell's rating and flag.
    fn redraw_all(mut self: Pin<&mut Self>) {
        let last = self.items.len() as i32 - 1;
        if last < 0 {
            return;
        }
        let (first, end) = (
            self.index(0, 0, &QModelIndex::default()),
            self.index(last, 0, &QModelIndex::default()),
        );
        let mut roles = QVector::<i32>::default();
        roles.append(ROLE_RATING);
        roles.append(ROLE_FLAG);
        roles.append(ROLE_LABEL);
        self.as_mut().data_changed(&first, &end, &roles);
    }

    pub fn rubber_begin(mut self: Pin<&mut Self>, additive: bool) {
        let base = if additive {
            self.selection.snapshot()
        } else {
            Default::default()
        };
        self.as_mut().rust_mut().rubber_base = Some(base);
    }

    pub fn rubber_to(
        mut self: Pin<&mut Self>,
        first_row: i32,
        last_row: i32,
        first_column: i32,
        last_column: i32,
        columns: i32,
    ) {
        let Some(base) = self.rubber_base.clone() else {
            return;
        };
        let columns = columns.max(1);
        let mut covered = Vec::new();
        for row in first_row.max(0)..=last_row {
            for column in first_column.max(0)..=last_column.min(columns - 1) {
                if let Some(item) = self.items.get((row * columns + column) as usize) {
                    covered.push(item.id);
                }
            }
        }
        self.as_mut().rust_mut().selection.set_over(&base, covered);
        self.selection_changed();
    }

    pub fn rubber_end(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().rubber_base = None;
    }

    pub fn rate_selection(mut self: Pin<&mut Self>, rating: i32) -> i32 {
        let Some(session) = session::current() else {
            return 0;
        };
        let rating = rating.clamp(0, 5) as u8;
        let rows: Vec<usize> = (0..self.items.len())
            .filter(|row| self.selection.contains(&self.items[*row].id))
            .collect();
        if rows.is_empty() {
            return 0;
        }
        let mut commands: Vec<Command> = self
            .photos_of(&rows)
            .into_iter()
            .map(|photo_id| Command::SetRating { photo_id, rating })
            .collect();
        // One photo is a plain edit; several are one action, one step of the history (D-096).
        let command = if commands.len() == 1 {
            commands.remove(0)
        } else {
            Command::Batch { commands }
        };
        let _ = session.engine.submit(command);
        // The cells show the new rating at once; the engine's own events confirm it.
        for row in &rows {
            self.as_mut().rust_mut().items[*row].rating = rating;
            // (A collapsed series' row stands for all its members.)
            for id in self.items[*row].photos() {
                self.as_mut().note_in_all(id, Some(rating), None, None);
                self.as_mut().ask(id, Some(rating), None, None);
            }
        }
        self.as_mut().redraw_all();
        rows.len() as i32
    }

    pub fn id_at(&self, row: i32) -> QString {
        usize::try_from(row)
            .ok()
            .and_then(|row| self.items.get(row))
            .map(|item| QString::from(item.id.to_string().as_str()))
            .unwrap_or_default()
    }

    pub fn row_of(&self, id: &QString) -> i32 {
        PhotoId::from_str(&id.to_string())
            .ok()
            .and_then(|id| {
                self.rows
                    .get(&id)
                    .or_else(|| self.hidden.get(&id).and_then(|rep| self.rows.get(rep)))
                    .copied()
            })
            .map_or(-1, |row| row as i32)
    }

    pub fn sync_photo(mut self: Pin<&mut Self>, id: &QString) {
        if let Ok(photo) = PhotoId::from_str(&id.to_string()) {
            self.as_mut().rust_mut().pending.remove(&photo);
        }
        self.refresh_photo(id);
    }

    pub fn refresh_photo(mut self: Pin<&mut Self>, id: &QString) {
        let Ok(id) = PhotoId::from_str(&id.to_string()) else {
            return;
        };
        // A photo that is not listed (one that has just entered the catalogue) waits for the reload; one a collapsed
        // series hides has no row but is in the list of everything.
        let row = self.rows.get(&id).copied();
        if row.is_none() && !self.all_pos.contains_key(&id) {
            return;
        }
        let Some(session) = session::current() else {
            return;
        };
        let Some(photo) = session
            .engine
            .read_catalogue()
            .ok()
            .and_then(|catalogue| catalogue.photo(&id).ok().flatten())
        else {
            return;
        };
        if let Some(pending) = self.pending.get(&id) {
            let fresh = pending.at.elapsed() < PENDING_FOR;
            let stale = pending.rating.is_some_and(|r| r != photo.effective_rating)
                || pending.flag.is_some_and(|f| f != photo.effective_flag)
                || pending
                    .label
                    .is_some_and(|l| l != label_code(photo.label.as_deref()));
            if fresh && stale {
                // An older state of a quick series of keys: the last one's own event follows.
                return;
            }
            self.as_mut().rust_mut().pending.remove(&id);
        }
        let label = label_code(photo.label.as_deref());
        self.as_mut().note_in_all(
            id,
            Some(photo.effective_rating),
            Some(photo.effective_flag),
            Some(label),
        );
        let Some(row) = row else {
            return;
        };
        if self.items[row].rating != photo.effective_rating
            || self.items[row].flag != photo.effective_flag
            || self.items[row].label != label
        {
            self.as_mut().rust_mut().items[row].rating = photo.effective_rating;
            self.as_mut().rust_mut().items[row].flag = photo.effective_flag;
            self.as_mut().rust_mut().items[row].label = label;
            self.redraw_photo(row);
        }
    }

    /// Redraws a cell's rating and flag.
    fn redraw_photo(mut self: Pin<&mut Self>, row: usize) {
        let index = self.index(row as i32, 0, &QModelIndex::default());
        let mut roles = QVector::<i32>::default();
        roles.append(ROLE_RATING);
        roles.append(ROLE_FLAG);
        roles.append(ROLE_LABEL);
        self.as_mut().data_changed(&index, &index, &roles);
    }

    /// Remembers what was asked of the engine for a photo, until the catalogue says the same.
    fn ask(
        mut self: Pin<&mut Self>,
        id: PhotoId,
        rating: Option<u8>,
        flag: Option<u8>,
        label: Option<u8>,
    ) {
        let (before_rating, before_flag, before_label) = self
            .pending
            .get(&id)
            .map_or((None, None, None), |p| (p.rating, p.flag, p.label));
        self.as_mut().rust_mut().pending.insert(
            id,
            Pending {
                rating: rating.or(before_rating),
                flag: flag.or(before_flag),
                label: label.or(before_label),
                at: Instant::now(),
            },
        );
    }

    pub fn info_at(&self, row: i32) -> QString {
        self.describe(row, false)
    }

    pub fn summary_at(&self, row: i32) -> QString {
        self.describe(row, true)
    }

    /// The photo's file and camera, and with `marks` its stars and flag too.
    fn describe(&self, row: i32, marks: bool) -> QString {
        let Some(item) = usize::try_from(row)
            .ok()
            .and_then(|row| self.items.get(row))
        else {
            return QString::default();
        };
        let Some(session) = session::current() else {
            return QString::default();
        };
        let photo = session
            .engine
            .read_catalogue()
            .ok()
            .and_then(|catalogue| catalogue.photo(&item.id).ok().flatten());
        let Some(photo) = photo else {
            return QString::default();
        };
        // The file, the camera and the stars, as many of them as there are.
        let stars = "\u{2605}".repeat(usize::from(photo.rating));
        let flag = match photo.effective_flag {
            1 => Some("\u{2714}".to_string()),
            2 => Some("\u{2716}".to_string()),
            _ => None,
        };
        let (stars, flag) = if marks {
            (stars, flag)
        } else {
            (String::new(), None)
        };
        let parts: Vec<String> = [Some(photo.filename), photo.camera, Some(stars), flag]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect();
        QString::from(parts.join(" \u{2014} ").as_str())
    }

    pub fn step(&self, current: i32, dx: i32, dy: i32, columns: i32) -> i32 {
        crate::gridmath::step(
            current.max(0) as usize,
            dx,
            dy,
            columns.max(1) as usize,
            self.items.len(),
        ) as i32
    }

    pub fn jump(&self, kind: &QString, current: i32, columns: i32, rows: i32) -> i32 {
        crate::gridmath::jump(
            &kind.to_string(),
            current.max(0) as usize,
            columns.max(1) as usize,
            rows.max(1) as usize,
            self.items.len(),
        ) as i32
    }

    pub fn set_rating(mut self: Pin<&mut Self>, row: i32, rating: i32) {
        let Some(session) = session::current() else {
            return;
        };
        let rating = rating.clamp(0, 5) as u8;
        let Some(id) = usize::try_from(row)
            .ok()
            .and_then(|row| self.items.get(row))
            .map(|item| item.id)
        else {
            return;
        };
        let _ = session.engine.submit(Command::SetRating {
            photo_id: id,
            rating,
        });
        // The cell shows the new rating at once; the engine's own event confirms it.
        self.as_mut().rust_mut().items[row as usize].rating = rating;
        self.as_mut().note_in_all(id, Some(rating), None, None);
        self.as_mut().ask(id, Some(rating), None, None);
        self.redraw_photo(row as usize);
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(item) = self.items.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_PHOTO_ID => QVariant::from(&QString::from(item.id.to_string().as_str())),
            ROLE_RATING => QVariant::from(&i32::from(item.rating)),
            ROLE_SELECTED => QVariant::from(&self.selection.contains(&item.id)),
            ROLE_FLAG => QVariant::from(&i32::from(item.flag)),
            ROLE_LABEL => QVariant::from(&QString::from(label_name(item.label))),
            ROLE_SERIES_ID => QVariant::from(&QString::from(
                item.series
                    .map(|s| s.to_string())
                    .unwrap_or_default()
                    .as_str(),
            )),
            ROLE_SERIES_SIZE => QVariant::from(&(item.series_size as i32)),
            ROLE_SERIES_TOTAL => QVariant::from(&(item.series_total as i32)),
            ROLE_SERIES_RESOLVED => QVariant::from(&item.series_resolved),
            ROLE_SERIES_OPEN => QVariant::from(&item.series_open),
            ROLE_SERIES_EDGE => QVariant::from(&i32::from(item.series_edge)),
            ROLE_MARKED => {
                QVariant::from(&item.photos().iter().any(|photo| self.marks.contains(photo)))
            }
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_PHOTO_ID, QByteArray::from("photoId"));
        roles.insert(ROLE_RATING, QByteArray::from("rating"));
        roles.insert(ROLE_SELECTED, QByteArray::from("selected"));
        roles.insert(ROLE_FLAG, QByteArray::from("flag"));
        roles.insert(ROLE_LABEL, QByteArray::from("colourLabel"));
        roles.insert(ROLE_SERIES_ID, QByteArray::from("seriesId"));
        roles.insert(ROLE_SERIES_SIZE, QByteArray::from("seriesSize"));
        roles.insert(ROLE_SERIES_TOTAL, QByteArray::from("seriesTotal"));
        roles.insert(ROLE_SERIES_RESOLVED, QByteArray::from("seriesResolved"));
        roles.insert(ROLE_SERIES_OPEN, QByteArray::from("seriesOpen"));
        roles.insert(ROLE_SERIES_EDGE, QByteArray::from("seriesEdge"));
        roles.insert(ROLE_MARKED, QByteArray::from("marked"));
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.items.len() as i32
    }
}

/// Qt::UserRole and the next ones.
const ROLE_NAME: i32 = 0x0100;
const ROLE_PATH: i32 = 0x0101;
const ROLE_OPENED: i32 = 0x0102;
const ROLE_FOUND: i32 = 0x0103;

/// The Rust side of the model.
#[derive(Default)]
pub struct KnownWorkspacesRust {
    count: i32,
    known: Vec<KnownWorkspace>,
}

impl cxx_qt::Initialize for qobject::KnownWorkspaces {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

impl qobject::KnownWorkspaces {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let known = Engine::known_workspaces(&crate::launch().dirs);
        let count = known.len() as i32;
        // SAFETY: every begin is followed by its end, with nothing in between that can fail.
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().known = known;
            self.as_mut().end_reset_model();
        }
        self.set_count(count);
    }

    pub fn path_at(&self, row: i32) -> QString {
        self.known
            .get(row as usize)
            .map(|k| QString::from(k.path.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }

    pub fn forget(mut self: Pin<&mut Self>, row: i32) {
        if let Some(entry) = self.known.get(row as usize) {
            let _ = Engine::forget_workspace(&crate::launch().dirs, entry.workspace_id);
        }
        self.as_mut().refresh();
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(entry) = self.known.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_NAME => QVariant::from(&QString::from(entry.name.as_str())),
            ROLE_PATH => QVariant::from(&QString::from(entry.path.to_string_lossy().as_ref())),
            ROLE_OPENED => QVariant::from(&QString::from(
                entry
                    .opened
                    .map(|when| when.to_string().chars().take(10).collect::<String>())
                    .unwrap_or_default()
                    .as_str(),
            )),
            ROLE_FOUND => QVariant::from(&entry.found),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_NAME, QByteArray::from("name"));
        roles.insert(ROLE_PATH, QByteArray::from("path"));
        roles.insert(ROLE_OPENED, QByteArray::from("opened"));
        roles.insert(ROLE_FOUND, QByteArray::from("found"));
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.known.len() as i32
    }
}
