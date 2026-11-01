@echo off
rem Decompiles the vanilla 26.3 jar with Vineflower into libs\sources\minecraft-26.3-sources.jar
rem (attached to the "minecraft-26.3" library in IntelliJ). Takes a few minutes.
setlocal EnableExtensions
cd /d "%~dp0.."
call "%~dp0jdk.cmd" || exit /b 1
if exist tools\decomp rmdir /s /q tools\decomp
mkdir tools\decomp

set "LIBS="
for %%j in (libs\mc\*.jar) do (
  echo %%~nxj | findstr /i natives >nul || call set "LIBS=%%LIBS%% -e=%%j"
)
"%JDK%\bin\java.exe" -Xmx6G -jar tools\vineflower-1.12.0.jar --thread-count=12 -dgs=1 -hdc=0 -rbr=0 -rsy=0 -asc=1 -iec=1 -jvn=1 -log=WARN ^
  %LIBS% libs\vanilla\minecraft-26.3.jar tools\decomp\
if errorlevel 1 ( echo [decompile] FAILED & exit /b 1 )

if not exist libs\sources mkdir libs\sources
if exist libs\sources\minecraft-26.3-sources.jar del libs\sources\minecraft-26.3-sources.jar
pushd tools\decomp
"%JDK%\bin\jar.exe" --create --file ..\..\libs\sources\minecraft-26.3-sources.jar --no-manifest com net
popd
rmdir /s /q tools\decomp
echo [decompile] ok: libs\sources\minecraft-26.3-sources.jar
