import os
import sys
import json
import urllib.request
import subprocess

sys.stdout.reconfigure(encoding='utf-8')

def get_github_token():
    p = subprocess.Popen(['git', 'credential', 'fill'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    out, err = p.communicate('protocol=https\nhost=github.com\n\n')
    creds = dict(line.split('=', 1) for line in out.splitlines() if '=' in line)
    return creds.get('password')

def main():
    token = get_github_token()
    if not token:
        print("ERROR: Could not get GitHub token from git credentials")
        sys.exit(1)

    tag_name = 'v2.8.0'
    release_name = 'ZOET v2.8.0 - استوديو فلاتر الصورة وتخصيص الترجمة ودعم يد التحكم'
    
    body_text = """# ZOET v2.8.0 🚀

تحديث رئيسي شامل يضيف استوديو فلاتر ومعالجة الصورة، تخصيص مظهر وألوان الترجمة، دعم كامل لأيدي التحكم، ومميزات حصرية لمشغل الفيديو وواجهة الكمبيوتر مع ضمان التوافق التام للتحديث المباشر من داخل التطبيق بدون أي تضارب حزم.

---

### ✨ أبرز الإضافات والمميزات في هذا الإصدار:

1. **🎨 استوديو فلاتر الفيديو (Video Picture Studio & Shaders):**
   - 4 أوضاع لونية ومعالجة فورية للصورة:
     - **عادي (Normal):** الصورة الأصلية بدون تعديل.
     - **أنمي (Anime Vivid):** ألوان مشبعة وتباين عالي يبرز تفاصيل الرسوم والأنمي.
     - **سينما (Cinema Warm):** درجات ألوان دافئة سينمائية وتجربة مشاهدة مريحة للعين.
     - **ظلال وظلام عميق (Shadow Deep):** تحسين مستويات السواد والتفاصيل في المشاهد المظلمة.
   - زر تبديل الفلاتر في واجهة المشغل مع اختصار لوحة المفاتيح: `V`.

2. **🔤 تخصيص ألوان وتنسيق الترجمة (Subtitle Customizer):**
   - إمكانية التبديل بين ألوان الترجمة المفضلة لديك:
     - 🟡 أصفر سينمائي (Yellow - كلاسيكي عالي الوضوح).
     - ⚪ أبيض ناصع (Pure White).
     - 🔵 سماوي (Cyan).
     - 🟢 أخضر مريح (Mint Green).
     - 🟠 كهرماني (Amber).
   - متوفر من خلال قائمة خيارات المشغل وعبر اختصار لوحة المفاتيح: `Shift + C`.

3. **🎮 دعم كامل لأيدي التحكم (Full Gamepad Support):**
   - دعم كامل لأجهزة تحكم Xbox و PlayStation والريموتات عبر الـ Gamepad API.
   - أزرار التحكم:
     - `A / Cross`: تشغيل / إيقاف مؤقت.
     - `D-Pad Left / Right`: تقديم وترجيع 10 ثوانٍ.
     - `Triggers (LT / RT)`: قفزات سريعة 30 ثانية.
     - `D-Pad Up / Down`: رفع وخفض الصوت.
     - `B / Circle`: إغلاق المشغل / العودة.

4. **🔊 مضخم الصوت الرقمي حتى 300% (300% Audio Boost):**
   - حل مشكلة المقاطع الصوتية الهادئة أو الترجمات الصوتية المنخفضة عبر معالج صوت Web Audio GainNode مدمج.

5. **📥 سحب وإفلات الترجمة الخارجية ومزامنة التوقيت:**
   - دعم سحب وإفلات ملفات `.srt` أو `.vtt` مباشرة فوق المشغل.
   - ضبط دقيق لتأخير وتقديم توقيت الترجمة باستخدام `[` و `]`.

6. **📸 التقاط لقطات شاشة سينمائية (Frame Snapshot):**
   - التقاط لقطة بالدقة الأصلية للفيديو بضغطة زر أو باختصار `S`.

7. **💻 واجهة كمبيوتر احترافية مخصصة (Desktop PC UI):**
   - شريط تحكم وإدارة نوافذ متكامل، استجابة عالية بالكامل عبر محرك Rust WebAssembly.

---

### 🛡️ معلومات التحديث والأمان:
- **معرف الحزمة (Application ID):** `com.krmzi.app`
- **رقم الإصدار الداخلي (versionCode):** `26` (أعلى من الإصدار السابق 25 لتمكين التحديث المباشر)
- **بصمة شهادة التوقيع (Signing Certificate SHA-256):** `2c6d74a5251577fcdfd8a9df651d52fb2641ff4ef1be6686c8266abed0a0cbf3` (متطابقة 100% مع التثبيتات السابقة لضمان عدم حدوث تضارب حزم)
- **تحديث تلقائي من داخل التطبيق (In-App Update):** مدعوم فورياً بمجرد فتح التطبيق والضغط على "تحديث الآن".
"""

    headers = {
        'Authorization': 'token ' + token,
        'Accept': 'application/vnd.github.v3+json',
        'User-Agent': 'ZOET-Release-Uploader'
    }

    # Check if release already exists
    release_info = None
    try:
        check_req = urllib.request.Request(f'https://api.github.com/repos/XRTM8/ZOET/releases/tags/{tag_name}', headers=headers)
        with urllib.request.urlopen(check_req) as resp:
            release_info = json.loads(resp.read().decode())
            print(f"Existing release found: ID {release_info['id']}")
    except urllib.error.HTTPError as e:
        if e.code == 404:
            print("Release does not exist yet. Creating...")
        else:
            raise

    if not release_info:
        payload = {
            'tag_name': tag_name,
            'name': release_name,
            'body': body_text,
            'draft': False,
            'prerelease': False
        }
        create_req = urllib.request.Request(
            'https://api.github.com/repos/XRTM8/ZOET/releases',
            data=json.dumps(payload).encode('utf-8'),
            headers={**headers, 'Content-Type': 'application/json'}
        )
        with urllib.request.urlopen(create_req) as resp:
            release_info = json.loads(resp.read().decode())
        print(f"Release created successfully! ID: {release_info['id']}")

    upload_url_template = release_info['upload_url']
    upload_url_base = upload_url_template.split('{')[0]

    # Upload files
    files_to_upload = [
        ('app-release.apk', 'release-output/app-release.apk', 'application/vnd.android.package-archive'),
        ('otium-play.apk', 'release-output/otium-play.apk', 'application/vnd.android.package-archive'),
        ('ZOET.exe', 'release-output/ZOET.exe', 'application/vnd.microsoft.portable-executable'),
    ]

    existing_assets = {a['name']: a['id'] for a in release_info.get('assets', [])}

    for asset_name, asset_path, mime_type in files_to_upload:
        if not os.path.exists(asset_path):
            print(f"Skipping {asset_name} (file not found)")
            continue

        if asset_name in existing_assets:
            print(f"Asset {asset_name} already exists (ID: {existing_assets[asset_name]}), deleting first...")
            del_req = urllib.request.Request(
                f"https://api.github.com/repos/XRTM8/ZOET/releases/assets/{existing_assets[asset_name]}",
                headers=headers,
                method='DELETE'
            )
            with urllib.request.urlopen(del_req) as resp:
                print(f"Deleted old asset {asset_name}")

        file_size = os.path.getsize(asset_path)
        print(f"Uploading {asset_name} ({file_size} bytes)...")
        upload_url = f"{upload_url_base}?name={asset_name}"
        
        with open(asset_path, 'rb') as f:
            file_data = f.read()

        upload_req = urllib.request.Request(
            upload_url,
            data=file_data,
            headers={
                **headers,
                'Content-Type': mime_type,
                'Content-Length': str(len(file_data))
            }
        )
        with urllib.request.urlopen(upload_req) as resp:
            upload_resp = json.loads(resp.read().decode())
            print(f"Uploaded {asset_name} successfully! Download URL: {upload_resp['browser_download_url']}")

    print("\n=== RELEASE v2.8.0 PUBLISHED AND ASSETS UPLOADED SUCCESSFULLY! ===")
    print(f"Release URL: {release_info['html_url']}")

if __name__ == '__main__':
    main()
