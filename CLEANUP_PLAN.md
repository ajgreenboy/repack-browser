# Homelab Cleanup Plan

## Disk Usage Summary
**Total Disk:** 80GB
**Used:** 53GB (71%)
**Available:** 23GB

---

## 🗑️ SAFE TO DELETE (6.7GB+)

### 1. Rust Build Artifacts
**Location:** `/home/al/fitgirl-scraper/target/`
**Size:** 2.6GB
**What:** Compiled Rust binaries and intermediate build files for the server
**Safe to delete?** ✅ YES - Can be rebuilt with `cargo build`
**Command:**
```bash
rm -rf /home/al/fitgirl-scraper/target/
```

### 2. Windows Client Build Artifacts
**Location:** `/home/al/fitgirl-scraper/client-agent/target/`
**Size:** 4.1GB
**What:** Compiled Windows client binaries and intermediate build files
**Safe to delete?** ✅ YES - Can be rebuilt with `cargo build --target x86_64-pc-windows-gnu`
**Command:**
```bash
rm -rf /home/al/fitgirl-scraper/client-agent/target/
```

### 3. Docker Build Cache
**Size:** 8.8GB reclaimable
**What:** Cached layers from Docker builds
**Safe to delete?** ✅ YES - Docker will re-download/rebuild as needed
**Command:**
```bash
docker builder prune -a -f
```

### 4. Unused Docker Images
**Size:** 15.9GB reclaimable
**What:** Old/unused Docker images (39 total, only 26 active)
**Safe to delete?** ✅ YES - Only removes unused images
**Command:**
```bash
docker image prune -a -f
```

**Total from safe deletions: ~31GB!**

---

## ⚠️ REVIEW BEFORE DELETING

### 5. Backup Dockerfile Copies
**Location:** `/home/al/fitgirl-scraper/`
**Files:**
- `Dockerfile.backup3` (4KB)
- `docker-compose.yml.backup` (4KB)
- `dockerignore`, `gitignore` (should be `.dockerignore`, `.gitignore`)

**Safe to delete?** ⚠️ PROBABLY - These look like old backups
**Recommendation:** Review first, then delete if confirmed unnecessary
**Command:**
```bash
cd /home/al/fitgirl-scraper
rm Dockerfile.backup3 docker-compose.yml.backup dockerignore gitignore
```

### 6. Old Client Binary
**Location:** `/home/al/fitgirl-scraper/client-agent/fitgirl-client.exe`
**Size:** 5MB
**What:** Old Windows client binary (before rename to repack-client.exe)
**Safe to delete?** ⚠️ YES - Superseded by releases/repack-client-windows-x64.exe
**Command:**
```bash
rm /home/al/fitgirl-scraper/client-agent/fitgirl-client.exe
```

### 7. fitgirl-browser-app Directory
**Location:** `/home/al/fitgirl-scraper/fitgirl-browser-app/`
**Size:** 22MB
**What:** Unknown - possibly old app directory?
**Safe to delete?** ❓ UNKNOWN - Need to investigate
**Command:** (after investigation)
```bash
ls -la /home/al/fitgirl-scraper/fitgirl-browser-app/
# Then decide
```

### 8. Docker User Data
**Location:** `/home/al/docker/data/`
**Size:** 3.7GB
**What:** Container persistent data
**Safe to delete?** ❌ NO - This is your actual application data
**Recommendation:** Do NOT delete unless you know what containers this is for

### 9. Docker Config
**Location:** `/home/al/docker/config/`
**Size:** 643MB
**What:** Container configuration files
**Safe to delete?** ❌ NO - Required for containers to run
**Recommendation:** Do NOT delete

---

## 📊 REPOSITORY CLEANUP

### Files That Should Be .gitignored But Aren't:
1. `client-agent/target/` - 4.1GB build artifacts ❌ IN GIT
2. `target/` - 2.6GB build artifacts ❌ IN GIT
3. `data/` - 13MB database ✅ Already ignored
4. `client-agent/fitgirl-client.exe` - 5MB old binary ❓ Check git

**Git Cleanup Needed:**
```bash
# Check if target dirs are tracked
cd /home/al/fitgirl-scraper
git ls-files | grep target/

# If they're tracked, remove from git (but keep locally)
git rm -r --cached target/
git rm -r --cached client-agent/target/
git commit -m "Remove build artifacts from git tracking"

# Verify .gitignore has these:
cat .gitignore | grep target
# Should see: **/target/
```

---

## 🚢 DOCKER COMPOSE STATUS

**Current Status:**
- Container is running (started 1 hour ago, healthy)
- Image is OLD (built before latest changes)
- Container name still shows old project name

**Does it need rebuilding?** ⚠️ YES

**Why:**
- Code has changed significantly (new architecture)
- Latest changes aren't in the running container
- Should rebuild with latest code

**To Rebuild:**
```bash
cd /home/al/fitgirl-scraper
docker-compose down
docker-compose build --no-cache
docker-compose up -d
```

**Note:** This will stop the current container, rebuild with latest code, and restart.

---

## 🪟 WINDOWS CLIENT REBUILD

**Current Status:**
- Latest build: `releases/repack-client-windows-x64.exe` (6.1MB)
- Built: ~10 minutes ago
- Includes: Latest changes (full download implementation)

**Does it need rebuilding?** ✅ NO - Already built with latest code

**To Rebuild (if needed):**
```bash
cd /home/al/fitgirl-scraper
./build-windows-client.sh
```

---

## 📝 RECOMMENDED ACTIONS

### Immediate (Safe, High Impact):
1. ✅ Delete Rust build artifacts: `rm -rf target/` (+2.6GB)
2. ✅ Delete Windows build artifacts: `rm -rf client-agent/target/` (+4.1GB)
3. ✅ Clean Docker cache: `docker builder prune -a -f` (+8.8GB)
4. ✅ Remove unused images: `docker image prune -a -f` (+15.9GB)

**Total space freed: ~31GB!**

### After Cleanup:
5. ✅ Rebuild Docker container with latest code
6. ✅ Remove build artifacts from git tracking
7. ⚠️ Investigate and remove old files (backup Dockerfiles, old binary)

### Optional:
8. Review docker/data/ to see if any container data can be cleaned up
9. Check docker logs: `docker logs fitgirl-browser` and prune if huge

---

## ⚡ QUICK CLEANUP SCRIPT

**Execute these in order:**
```bash
# 1. Clean Rust builds
cd /home/al/fitgirl-scraper
rm -rf target/
rm -rf client-agent/target/

# 2. Clean Docker
docker builder prune -a -f
docker image prune -a -f

# 3. Remove old files
rm client-agent/fitgirl-client.exe
rm Dockerfile.backup3 docker-compose.yml.backup

# 4. Rebuild Docker with latest code
docker-compose down
docker-compose build --no-cache
docker-compose up -d

# 5. Check results
df -h /home
docker ps
```

**Expected Result:** ~31GB freed, container running with latest code

---

**Generated:** 2026-02-07
**Disk Usage Before:** 53GB/80GB (71%)
**Estimated After Cleanup:** ~22GB/80GB (28%)
**Space Freed:** ~31GB
