# Quick Start Guide

## What's Included

This package contains everything you need to run FitGirl Browser on Windows!

```
fitgirl-browser-windows-complete/
├── src/                   ← Rust source code
│   ├── main.rs           ← Main server
│   ├── db.rs             ← Database
│   ├── realdebrid.rs     ← Real-Debrid API
│   └── scraper.rs        ← Web scraper
├── frontend/             ← Web interface
│   ├── index.html
│   └── app.js
├── Cargo.toml            ← Rust project file
├── run.bat               ← Launcher script
├── README.md             ← Full documentation
└── BUILD.md              ← Detailed build instructions
```

## Step 1: Install Rust

1. Go to: https://rustup.rs/
2. Download and run `rustup-init.exe`
3. Follow the installer (just press Enter for defaults)
4. **Restart your Command Prompt** after installation

Verify:
```cmd
rustc --version
cargo --version
```

## Step 2: Build the App

Open Command Prompt in this folder and run:

```cmd
cargo build --release
```

This will take 5-10 minutes the first time. Grab a coffee! ☕

## Step 3: Set Up Your App

After building, create your app folder:

```cmd
mkdir fitgirl-browser-app
cd fitgirl-browser-app

REM Copy the executable
copy ..\target\release\fitgirl-browser.exe .

REM Copy the frontend
xcopy ..\frontend frontend\ /E /I

REM Copy the launcher
copy ..\run.bat .
```

## Step 4: Add Your Real-Debrid API Key

1. Get your API key from: https://real-debrid.com/apitoken
2. Right-click `run.bat` → Edit with Notepad
3. Find this line:
   ```
   set RD_API_KEY=YOUR_REAL_DEBRID_API_KEY_HERE
   ```
4. Replace `YOUR_REAL_DEBRID_API_KEY_HERE` with your actual key
5. Save and close

## Step 5: Run It!

Double-click `run.bat`

The app will:
- Start the server
- Open your browser to http://localhost:3000
- You're ready to go!

## Troubleshooting

**"rustc not found":**
- Make sure Rust is installed
- Restart Command Prompt
- Try: `refreshenv` (if you have Chocolatey)

**Build fails with linker error:**
- Install Visual Studio Build Tools
- Download from: https://visualstudio.microsoft.com/downloads/
- Select "Desktop development with C++"

**"Cannot find main.rs":**
- Make sure you're in the `fitgirl-browser-windows-complete` folder
- Check that `src/main.rs` exists

**App won't start:**
- Check if port 3000 is already in use
- Look for error messages in the Command Prompt window

## Next Steps

Once running:
1. Upload a CSV of games OR click "Re-scrape Site"
2. Browse your games
3. Click a game → "Add to Real-Debrid"
4. Download links appear!
5. Enjoy!

## Need Help?

Check `README.md` for full documentation or `BUILD.md` for detailed build instructions.
