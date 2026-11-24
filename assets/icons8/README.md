# Icons8 SF Regular icons

Selected app icons use the [Apple SF Regular collection by Icons8](https://icons8.com/icons/all--os-macos).
They are free 100px PNGs with attribution, embedded in the application. The app's
tray's **About / credits** menu links to [Icons8](https://icons8.com/).

These assets are **not covered by this project's MIT license**. See the
[Icons8 license](https://icons8.com/license) and
[full agreement](https://icons8.com/vue-static/landings/pricing/icons8-license.pdf).
Original files may not be redistributed as a standalone graphics collection.
Raw PNGs are therefore ignored by Git and excluded from release ZIPs. Build-time
downloads and their hashes are listed in `manifest.json`.

Run `python scripts/fetch-icons.py` before a source build. The EXE embeds the
selected images; end users do not need Python or an icon download at runtime.
