# Building FitGirl Browser for Windows

## Prerequisites

1. **Install Rust** (if not already installed)
   - Download from: https://rustup.rs/
   - Run the installer and follow prompts
   - Restart your terminal/Command Prompt after installation

2. **Verify Rust installation**
   ```cmd
   rustc --version
   cargo --version
   ```

## Build Instructions

### Option 1: Quick Build (Debug - Faster to compile)

```cmd
cargo build
```

The executable will be at: `target\debug\fitgirl-browser.exe`

### Option 2: Release Build (Optimized - Recommended for actual use)

```cmd
cargo build --release
```

The executable will be at: `target\release\fitgirl-browser.exe`

This will take longer to compile but runs much faster!

## Setup After Building

1. **Create the app folder structure:**
   ```cmd
   mkdir fitgirl-browser-app
   cd fitgirl-browser-app
   ```

2. **Copy files:**
   ```cmd
   REM Copy the built executable
   copy ..\target\release\fitgirl-browser.exe .

   REM Copy the frontend folder
   xcopy ..\frontend frontend\ /E /I

   REM Copy the run script
   copy ..\run.bat .
   ```

3. **Edit run.bat:**
   - Open `run.bat` in Notepad
   - Replace `YOUR_REAL_DEBRID_API_KEY_HERE` with your actual API key
   - Get your API key from: https://real-debrid.com/apitoken

4. **Run the app:**
   ```cmd
   run.bat
   ```

   Or double-click `run.bat` in File Explorer!

## What Gets Created

```
fitgirl-browser-app/
├── fitgirl-browser.exe    ← The app
├── run.bat                ← Launcher script
├── frontend/              ← Web interface files
│   ├── index.html
│   └── app.js
└── data/                  ← Created automatically
    └── games.db          ← Database (created on first run)
```

## Usage

1. Double-click `run.bat`
2. Your browser will open to http://localhost:3000
3. Upload a CSV or run the scraper
4. Start downloading games!

## Troubleshooting

**"cargo not found":**
- Make sure Rust is installed
- Restart your terminal/Command Prompt
- Try opening a new Command Prompt window

**Build fails with linker error:**
- Install Visual Studio Build Tools: https://visualstudio.microsoft.com/downloads/
- Select "Desktop development with C++" during installation

**"Cannot find frontend files":**
- Make sure the `frontend` folder is in the same directory as the .exe
- Check that `index.html` and `app.js` are inside the `frontend` folder

**"Database error":**
- The app will create a `data` folder automatically
- If issues persist, delete the `data` folder and restart

**"Real-Debrid not working":**
- Make sure you edited `run.bat` with your real API key
- Get your key from: https://real-debrid.com/apitoken
- The key should look like: `ABC123DEF456...` (long alphanumeric string)

## Alternative: Set API Key as Environment Variable

Instead of editing `run.bat`, you can set it system-wide:

1. Press Win + X, select "System"
2. Click "Advanced system settings"
3. Click "Environment Variables"
4. Under "User variables", click "New"
5. Variable name: `RD_API_KEY`
6. Variable value: Your Real-Debrid API key
7. Click OK
8. Restart Command Prompt

Then you can just run:
```cmd
fitgirl-browser.exe
```

Without needing the batch file!
