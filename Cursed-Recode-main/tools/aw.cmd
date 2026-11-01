@echo off
rem Applies src\main\resources\cursed-recode.accesswidener to the game jar:
rem   libs\vanilla\minecraft-26.3.jar  ->  libs\minecraft\minecraft-26.3.jar
rem Run after editing the .accesswidener (build.cmd / run.cmd call it automatically).
rem In IntelliJ press "Reload from disk" / wait for sync afterwards so the IDE sees the new jar.
setlocal EnableExtensions
cd /d "%~dp0.."
call "%~dp0jdk.cmd" || exit /b 1
"%JDK%\bin\java.exe" -cp "libs\fabric\*" tools\ApplyAccessWidener.java ^
  libs\vanilla\minecraft-26.3.jar src\main\resources\cursed-recode.accesswidener libs\minecraft\minecraft-26.3.jar
exit /b %errorlevel%
