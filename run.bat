@echo off
echo ============================================
echo   FitGirl Game Browser
echo ============================================
echo.

REM Set your Real-Debrid API key here
set RD_API_KEY=api-key

REM Download settings
set DOWNLOAD_DIR=C:\Games\FitGirl
set AUTO_EXTRACT=true
set DELETE_ARCHIVES=false

REM Check if API key is set
if "%RD_API_KEY%"=="YOUR_REAL_DEBRID_API_KEY_HERE" (
    echo WARNING: Real-Debrid API key not set!
    echo Edit this file and replace YOUR_REAL_DEBRID_API_KEY_HERE with your actual key
    echo Get it from: https://real-debrid.com/apitoken
    echo.
    echo Press any key to continue anyway, or close this window to exit...
    pause >nul
)

echo Starting FitGirl Browser...
echo.
echo The app will open in your browser at: http://localhost:3000
echo.
echo To stop the server, close this window or press Ctrl+C
echo.

REM Start the app
fitgirl-browser.exe

pause
