@echo off
rem Compiles and starts Minecraft 26.3 + Fabric Loader (dev mode) with the mod from out\production\cursed-recode, as
rem players get it: one program, CursedRecode.exe (native\src\bin\host.rs) - the window before the game at once and the
rem game's Java in the same process. Extra arguments are passed to the game, e.g.  run.cmd --quickPlayMultiplayer mc.example.net
setlocal EnableExtensions
cd /d "%~dp0"
call "%~dp0build.cmd" nojar || exit /b 1
rem the DLL and the programs built together (as IntelliJ's IdeaBoot does): a DLL newer than CursedRecode.exe and
rem cursed_twin.exe cannot join their window before the game, which then plays its card over the menu for ever
pushd native
cargo build --release
if errorlevel 1 ( popd & echo [run] cargo build FAILED & exit /b 1 )
popd
call "%~dp0tools\jdk.cmd" || exit /b 1
if not exist run mkdir run
cd run

start "" /wait "..\native\target\release\CursedRecode.exe" --jdk "%JDK%" -Xmx4G -Xss1M -XX:StackShadowPages=32 ^
  --enable-native-access=ALL-UNNAMED --add-exports java.base/jdk.internal.misc=ALL-UNNAMED ^
  -Dfabric.development=true -Dfabric.log.disableAnsi=true -Dlog4j2.formatMsgNoLookups=true ^
  "-Dlog4j.configurationFile=..\tools\log4j2-dev.xml" ^
  -cp "..\libs\fabric\*;..\libs\minecraft\*;..\libs\mc\*;..\libs\fabric-api\*;..\out\production\cursed-recode" ^
  net.fabricmc.loader.impl.launch.knot.KnotClient ^
  --username Dev --assetIndex 34 --assetsDir "%APPDATA%\.minecraft\assets" --gameDir . --versionType "Cursed Recode" %*
