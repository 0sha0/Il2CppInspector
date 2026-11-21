# Game Plugins

Each `src/Plugins/<GameId>/` project builds `Il2CppInspector.Plugin.<GameId>.dll` and is packaged directly into `plugins/` (with shared native libraries under `runtime/<rid>/native/`) on build/publish. Plugins decode game metadata/binaries and supply verified runtime layout hooks; reflection, declarations, dependency ordering and all outputs stay in Common.

```bash
dotnet build Il2CppInspector.slnx -c Release
dotnet artifacts/bin/Il2CppInspector.CLI/release/Il2CppInspector.dll --game NAME_REGION_PLATFORM_VERSION -i GameAssembly.dll -m global-metadata.dat -o output/game -t IDA
```

`--game` takes an installed `GameId` (case-insensitive; `x.x.x` matches multiple versions). When omitted, metadata and embedded-metadata binaries are matched automatically; ambiguous matches require `--game`, and unmatched files use the stock loader. `-t IDA` writes both GUI `il2cpp.py` and idat terminal `il2cpp_t.py` to `Script_IDA/`, which supports `tqdm` progress, `q` to save and exit, and `.progress.json` checkpoint resume. Each script target gets its own `Script_<target>/` folder with `il2cpp.h` and `il2cpp.json`.

| Game | Supported ID |
| --- | --- |
| Arknights | `Arknights_CN_Windows_2.7.91` |
| Cygames | `Cy_JP_Windows_x.x.x` |
| BH3 | `BH3_CN_Windows_9.1.0` |
| Genshin | `Genshin_CN_Windows_7.1.0`, `Genshin_CN_Windows_7.2.0` |
| HSR | `HSR_OS_Windows_4.6.0`, `HSR_CN_Windows_4.6.51` |
| ZZZ | `ZZZ_CN_Windows_3.2.0`, `ZZZ_CN_Windows_3.3.0` |
| Endfield | `Endfield_CN_Android_x.x.x`, `Endfield_CN_Windows_1.6.3` |
| TOT | `TOT_CN_Android_6.1.0`, `TOT_OS_Android_5.8.1` |
| Love and Deepspace | `LAD_CN_Android_6.0.0` |
| Limbus Company | `LimbusCompany_OS_Windows_1.119.1` |
| Justice (逆水寒) | `NSH_OS_Windows_20260731072817UTC` |

MORAX plugins (BH3, Genshin, HSR, ZZZ) import `Shared/Morax/Morax.props` and keep only build-specific keys, offsets, record readers and verified layout differences. Multi-version games link shared sources across sibling directories instead of duplicating files. Cy embeds metadata in the binary and does not require `-m`. NSH accepts `-m common.dat` directly on the CLI.
