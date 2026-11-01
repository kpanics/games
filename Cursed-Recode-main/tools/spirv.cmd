@echo off
rem Compiles native\shaders\launch, native\shaders\gui and native\shaders\chrome into their spv folders (the DLL carries them). Run after editing those shaders.
call "%~dp0jdk.cmd" || exit /b 1
"%JDK%\bin\java.exe" -cp "%~dp0..\libs\mc\*" "%~dp0Spirv.java" "%~dp0..\native\shaders\launch" || exit /b 1
"%JDK%\bin\java.exe" -cp "%~dp0..\libs\mc\*" "%~dp0Spirv.java" "%~dp0..\native\shaders\gui" || exit /b 1
"%JDK%\bin\java.exe" -cp "%~dp0..\libs\mc\*" "%~dp0Spirv.java" "%~dp0..\native\shaders\chrome"
