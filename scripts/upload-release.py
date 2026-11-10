import os
import sys
import json
import hashlib
import urllib.request
import urllib.error
import subprocess
import time

sys.stdout.reconfigure(encoding='utf-8')

TAG_NAME = 'v2.9.4'
RELEASE_NAME = 'ZOET v2.9.4 - أوفرهول واجهة وأدوات مشغل الفيديو'
RELEASE_BODY = """# ZOET v2.9.4 🎬

أوفرهول متكامل لواجهة وتجربة مشغل الفيديو على الهاتف (عمودي وأفقي)، والكمبيوتر، والتلفزيون لحل تداخل العناصر والقص وإضافة كافة أدوات التحكم بشكل نظيف واحترافي.

---

### ✨ أبرز التحسينات والإصلاحات في المشغل:

1. **📱 حل التداخل والقص في مشغل الهاتف العمودي (`Portrait`):**
   - إلغاء الصناديق الداكنة الثقيلة المحيطة بالعنوان والوقت والاكتفاء بتدرج سينمائي ناعم أعلى وأسفل الإطار لتفريغ المساحة.
   - فصل منطقة أزرار التشغيل الوسطى تماماً عن الشريط السفلي ومنع مرور شريط التقديم عبر زر التشغيل.
   - إبعاد الأزرار السفلية عن منطقة الترجمة العربية (`video::cue`) ووضع الإعدادات الثانوية داخل زر **أدوات المشغل (`···`)** بدلاً من خروج الأيقونات خارج حافة الشاشة.

2. **🛠️ نافذة «أدوات المشغل المتقدمة» وقفل اللمس:**
   - إتاحة التحكم بـ **سرعة التشغيل (`0.5× – 2×`)**، **مضخم الصوت حتى `300%`**، **أبعاد العرض (`contain / cover / fill`)**، **فلاتر الصورة**، و**إزاحة/حجم/لون واستيراد ملفات الترجمة (`.srt / .vtt`)** بلمسة واحدة على جميع المنصات.
   - إضافة زر **قفل شاشة اللمس (`Touch Lock`)** لمنع النقرات العرضية مع شريط صغير لإلغاء القفل عند لمس الشاشة.

3. **👆 إيماءات السحب وتجربة العرض الأفقي وقائمة الحلقات:**
   - تفعيل السحب العمودي الحقيقي (يمين الشاشة للصوت، ويسارها لتعتيم الإضاءة السلس عبر طبقة `#playerBrightnessOverlay`).
   - تحسين قائمة الحلقات أسفل المشغل بإزالة الحافة الجانبية المشوهة في البطاقة النشطة، وقفل وضع المشاهدة الأفقية للموبايل (`Landscape`) مع إتاحة زر الحلقات السريع.

---

### 📦 الملفات المرفقة:
- **`app-release.apk` / `otium-play.apk`**: نسخة Android (`com.krmzi.app` · `versionCode 30`) موقّعة بالمفتاح المستقر ومتوافقة مع الترقية المباشرة.
- **`ZOET.exe`**: نسخة Windows الأصلية المستقلة (Tauri v2).

---

### 🛡️ معلومات التثبيت والتحقق:
- **معرف الحزمة:** `com.krmzi.app` | **versionCode:** `30` | **SHA-256 للشهادة:** `2c6d74a5251577fcdfd8a9df651d52fb2641ff4ef1be6686c8266abed0a0cbf3`
- **حالة التحقق:** اجتاز البناء اختبارات Rust وتوقيع APK ومحاذاة `16KB` واختبارات المتصفح للموبايل العمودي/الأفقي وسطح المكتب والتلفاز والـ PWA عبر محاكي المتصفح؛ لم يُجرَ اختبار فعلي على جهاز Android أو هاتف حقيقي لهذا البناء بعد.
"""

def get_github_token():
    p = subprocess.Popen(['git', 'credential', 'fill'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    out, err = p.communicate('protocol=https\nhost=github.com\n\n')
    creds = dict(line.split('=', 1) for line in out.splitlines() if '=' in line)
    return creds.get('password')

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(65536), b''):
            h.update(chunk)
    return h.hexdigest()

def ensure_release(headers):
    tag_url = f'https://api.github.com/repos/XRTM8/ZOET/releases/tags/{TAG_NAME}'
    check_req = urllib.request.Request(tag_url, headers=headers)
    try:
        with urllib.request.urlopen(check_req) as resp:
            release_info = json.loads(resp.read().decode())
            print(f"Existing release found: ID {release_info['id']} ({TAG_NAME})")
            # Keep title & body updated
            patch_data = json.dumps({
                'name': RELEASE_NAME,
                'body': RELEASE_BODY,
                'draft': False,
                'prerelease': False,
            }).encode('utf-8')
            patch_req = urllib.request.Request(
                f'https://api.github.com/repos/XRTM8/ZOET/releases/{release_info["id"]}',
                data=patch_data,
                headers={**headers, 'Content-Type': 'application/json'},
                method='PATCH',
            )
            with urllib.request.urlopen(patch_req) as presp:
                return json.loads(presp.read().decode())
    except urllib.error.HTTPError as e:
        if e.code != 404:
            raise
    print(f"Creating release {TAG_NAME} on XRTM8/ZOET (target main)...")
    create_data = json.dumps({
        'tag_name': TAG_NAME,
        'target_commitish': 'main',
        'name': RELEASE_NAME,
        'body': RELEASE_BODY,
        'draft': False,
        'prerelease': False,
    }).encode('utf-8')
    create_req = urllib.request.Request(
        'https://api.github.com/repos/XRTM8/ZOET/releases',
        data=create_data,
        headers={**headers, 'Content-Type': 'application/json'},
        method='POST',
    )
    with urllib.request.urlopen(create_req) as resp:
        return json.loads(resp.read().decode())

def main():
    token = get_github_token()
    if not token:
        print("ERROR: Could not get GitHub token from git credentials")
        sys.exit(1)

    headers = {
        'Authorization': 'token ' + token,
        'Accept': 'application/vnd.github.v3+json',
        'User-Agent': 'ZOET-Release-Uploader'
    }

    release_info = ensure_release(headers)
    print(f"Release ready: ID {release_info['id']}")

    upload_url_template = release_info['upload_url']
    upload_url_base = upload_url_template.split('{')[0]

    files_to_upload = [
        ('app-release.apk', 'release-output/app-release.apk', 'application/vnd.android.package-archive'),
        ('otium-play.apk', 'release-output/otium-play.apk', 'application/vnd.android.package-archive'),
        ('ZOET.exe', 'release-output/ZOET.exe', 'application/vnd.microsoft.portable-executable'),
    ]

    req = urllib.request.Request(f'https://api.github.com/repos/XRTM8/ZOET/releases/{release_info["id"]}', headers=headers)
    with urllib.request.urlopen(req) as resp:
        current_release = json.loads(resp.read().decode())
    existing_assets = {a['name']: a['id'] for a in current_release.get('assets', [])}

    for asset_name, asset_path, mime_type in files_to_upload:
        if not os.path.exists(asset_path):
            print(f"Skipping {asset_name} (file not found)")
            continue

        if asset_name in existing_assets:
            print(f"Asset {asset_name} already exists (ID: {existing_assets[asset_name]}), deleting first...")
            del_cmd = [
                'curl.exe', '--ssl-no-revoke', '-s', '-X', 'DELETE',
                '-H', f'Authorization: token {token}',
                f'https://api.github.com/repos/XRTM8/ZOET/releases/assets/{existing_assets[asset_name]}'
            ]
            subprocess.run(del_cmd, check=True)
            time.sleep(1)

        local_sha = sha256_file(asset_path)
        file_size = os.path.getsize(asset_path)
        print(f"\nUploading {asset_name} ({file_size} bytes, sha256:{local_sha})...")
        upload_url = f"{upload_url_base}?name={asset_name}"

        curl_cmd = [
            'curl.exe',
            '--ssl-no-revoke',
            '-X', 'POST',
            '-H', f'Authorization: token {token}',
            '-H', f'Content-Type: {mime_type}',
            '--retry', '5',
            '--retry-delay', '3',
            '--data-binary', f'@{os.path.abspath(asset_path)}',
            upload_url
        ]

        proc = subprocess.run(curl_cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            print(f"ERROR: curl failed with code {proc.returncode}:\n{proc.stderr}")
            sys.exit(proc.returncode)

        resp_data = json.loads(proc.stdout)
        digest = resp_data.get('digest') or ''
        print(f"Uploaded {asset_name}: {resp_data.get('browser_download_url')} (remote digest: {digest})")
        if digest and digest != f"sha256:{local_sha}":
            print(f"ERROR: Digest mismatch for {asset_name}! expected sha256:{local_sha}, got {digest}")
            sys.exit(1)

    print(f"\n=== RELEASE {TAG_NAME} PUBLISHED AND ASSETS VERIFIED SUCCESSFULLY! ===")
    print(f"Release URL: {release_info['html_url']}")

if __name__ == '__main__':
    main()
