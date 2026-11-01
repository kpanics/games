@echo off
rem Compiles src\main\java with plain javac (no Gradle / Maven) into out\production\cursed-recode
rem and packs build\cursed-recode-<version>.jar.   Usage: build.cmd [nojar]
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0"
call "%~dp0tools\jdk.cmd" || exit /b 1

set "VERSION=0.1.0"
set "OUT=out\production\cursed-recode"
set "CP=libs\fabric\*;libs\minecraft\*;libs\mc\*;libs\fabric-api\*"

if exist "%OUT%" rmdir /s /q "%OUT%"
mkdir "%OUT%" >nul 2>&1
rem javac @argfile: пути в кавычках, backslash там — экранирование, поэтому меняем на /
(for /f "delims=" %%f in ('dir /s /b src\main\java\*.java') do @(set "f=%%f" & echo "!f:\=/!")) > out\sources.txt

call "%~dp0tools\aw.cmd" || exit /b 1
echo [build] javac ...
"%JDK%\bin\javac.exe" -encoding UTF-8 -g -parameters --release 25 -proc:none -Xlint:-options -cp "%CP%" -d "%OUT%" @out\sources.txt
if errorlevel 1 ( echo [build] FAILED & exit /b 1 )
xcopy /s /e /y /q src\main\resources "%OUT%\" >nul
if /i "%~1"=="nojar" ( echo [build] ok ^(classes only^) & exit /b 0 )

if not exist build mkdir build
"%JDK%\bin\jar.exe" --create --file "build\cursed-recode-%VERSION%.jar" -C "%OUT%" .
if errorlevel 1 ( echo [build] jar FAILED & exit /b 1 )
echo [build] ok: build\cursed-recode-%VERSION%.jar
