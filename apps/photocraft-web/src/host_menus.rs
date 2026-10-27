//! Evora build (patch LUM-09): the menu items the Evora build hides under the host bridge.
//!
//! Features the first version of Evora Lumbra leaves out, or that are reachable in the browser and
//! do nothing there: batch processing and droplets, scripts that read files or folders from disk,
//! exports to a folder, printing, the video timeline and video layers, plug-ins, variables and
//! data sets, Exit (the host shell closes the editor), and, since upstream v0.16.0, Help › Check
//! for Updates (no updater in the browser; its window names upstream's release page). They leave
//! the menus, the command palette and `diag.menus`, and the engine ignores them, shortcuts included
//! (`photocraft_ui_egui::menus::set_host_hidden`). Each id is one of the menu catalogue or of the
//! engine's command table of this version; `diag.menus` reports the ones it removed (`masked`),
//! which the smoke test compares with what it expects. Without the bridge, the menus are
//! upstream's.

/// The hidden ids, grouped as listed above.
pub const HIDDEN: [&str; 39] = [
    // Batch processing and droplets (no file system in the browser).
    "file.automate.batch",
    "file.automate.createDroplet",
    "file.automate.contactSheetII",
    "file.automate.lensCorrection",
    "file.scripts.imageProcessor",
    // Scripts that read their files from disk (Load Files into Stack, Statistics): the browser
    // hands over contents, never paths or folders.
    "file.scripts.loadFilesIntoStack",
    "file.scripts.statistics",
    // Exports to a folder.
    "file.export.layersToFiles",
    "file.export.artboardsToFiles",
    "file.export.layerCompsToFiles",
    "file.export.renderVideo",
    "file.generate.imageAssets",
    "file.package",
    // Printing.
    "file.print",
    "file.printOneCopy",
    // Video timeline and video layers.
    "window.panel.timeline",
    "file.import.videoFramesToLayers",
    "layer.videoLayers.newBlankVideoLayer",
    "layer.videoLayers.newVideoLayerFromFile",
    "layer.videoLayers.insertBlankFrame",
    "layer.videoLayers.duplicateFrame",
    "layer.videoLayers.deleteFrame",
    "layer.videoLayers.replaceFootage",
    "layer.videoLayers.interpretFootage",
    "layer.videoLayers.showAlteredVideo",
    "layer.videoLayers.restoreFrame",
    "layer.videoLayers.restoreAllFrames",
    "layer.videoLayers.reloadFrame",
    "layer.videoLayers.rasterize",
    "layer.rasterize.video",
    // Plug-ins.
    "plugin.install",
    "edit.preferences.plugIns",
    // Variables and data sets.
    "image.variables.define",
    "image.variables.dataSets",
    "image.applyDataSet",
    "file.import.variableDataSets",
    "file.export.dataSetsAsFiles",
    // Exit: the host shell closes the editor (eframe's web backend ignores the window close).
    "file.exit",
    // Help › Check for Updates (upstream v0.16.0): releases are the host's, never upstream's page.
    "help.checkForUpdates",
];

/// Hide [`HIDDEN`] (under the bridge only).
pub fn install() {
    photocraft_ui_egui::menus::set_host_hidden(&HIDDEN);
}
