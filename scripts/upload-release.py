import os
import sys
import json
import hashlib
import urllib.request
import urllib.error
import subprocess
import time

sys.stdout.reconfigure(encoding='utf-8')

TAG_NAME = 'v3.5.0'
RELEASE_NAME = 'ZOET v3.5.0 - إعادة بناء نسخة التلفزيون والشاشات الكبيرة من الصفر (Ground-Up Leanback Overhaul)'
RELEASE_BODY = """# ZOET v3.5.0 🎬

أوفرهول وإعادة بناء جذرية وشاملة لنسخة شاشات وأجهزة التلفزيون (Android TV / Leanback)، مع صفحة تفاصيل سينمائية كاملة للحلقات والمواسم، نظام تحكم حتمي ودقيق بالريموت في المشغل، إزالة التقطيع والكلجات الرسومية، والواجهة البريميوم.

---

### 📺 1. إعادة بناء صفحة تفاصيل المسلسل وشبكة الحلقات (Two-Pane Leanback Architecture):
- **صفحة سينمائية كاملة تمتد على كامل الشاشة (94vw × 90vh):** إلغاء نظام النوافذ المنبثقة الصغيرة (Bottom Sheet) وحذف زر الإغلاق "X" نهائياً، والاعتماد على زر الرجوع (Back) الطبيعي في الريموت.
- **جناح معلومات المسلسل (الأيمن - 350px):** بوستر العمل بدقة عالية، التقييم، سنة الإنتاج، ملخص القصة، وزر كبير مباشر "▶ متابعة المشاهدة (الحلقة X)" أو التبديل بين النسخ (مترجم / مدبلج).
- **جناح الحلقات الشامل (الأيسر - 65% من الشاشة):**
  - شريط علوي لتبويبات المواسم (الموسم 1، 2...).
  - شبكة الحلقات تملأ الارتفاع المتبقي بالكامل في عمودين أو 3 أعمدة متناسقة، لعرض 8 إلى 12 حلقة واضحة ومقروءة دفعة واحدة في نفس الوقت دون أي انضغاط أو مساحات ميتة.

---

### 🎮 2. نظام تحكم المشغل المخصص للتلفزيون (TV Player OSD & Controller State Machine):
- **معالجة زر OK / Enter:**
  - عند إخفاء شريط التحكم: الضغط على OK يُظهر شريط التحكم فوراً مع تركيز إجباري وحتمي على زر التشغيل/الإيقاف (Play/Pause) في المنتصف، دون الضغط بالخطأ على أي قائمة أخرى أو فتح الحلقات.
  - عند ظهور الشريط: ينفذ أمر العنصر المحدد.
- **الأسهم D-Pad:**
  - يميناً ويساراً: تقديم وترجيع مباشر 10 ثوانٍ مع ظهور مؤشر رقمي فوري (`+10` / `-10`) وشريط التقدم دون فتح أي قوائم جانبية.
  - لأعلى: يُظهر شريط التحكم ويركز على زر التشغيل.
  - لأسفل: يُظهر شريط التحكم ويركز فوراً على زر قائمة الحلقات.
- **تنظيف واجهة التلفزيون:** إخفاء أزرار الهواتف (ملء الشاشة، PiP، قفل اللمس) التي لا حاجة لها على التلفزيون.

---

### ⚡ 3. القضاء التام على التقطيع وسقوط الإطارات (Zero-Jank 60 FPS):
- حذف فلاتر التمويه الزجاجي المعقدة `backdrop-filter: blur` بالكامل في وضع التلفزيون لتخفيف الحمل عن معالج الرسوميات (GPU)، واستبدالها بخلفيات داكنة ناعمة وخفيفة جداً.
- إطار تركيز أبيض ناصع عالي التباين (`outline: 3.5px solid #ffffff`) يعطي وضوحاً كاملاً لمكان المؤشر مع سرعة استجابة فورية.
- ضبط شبكة البوسترات الرئيسية لتتسع لـ 5 إلى 6 بوسترات متناسقة في كل صف مع ظهور صفين كاملين على الشاشة.

---

### 📦 الملفات المرفقة:
- **`app-release.apk` / `otium-play.apk`**: نسخة Android الرسمية للهواتف والتلفزيون (`com.krmzi.app` · `versionCode 41`) موقّعة بالمفتاح المستقر ومحاذاة 16KB لجميع المعالجات.
- **نسخة الويب والحاسوب**: متاحة ومثبتة مباشرة عبر الرابط [https://xrtm8.github.io/ZOET-Web/](https://xrtm8.github.io/ZOET-Web/)

---

### 🛡️ معلومات التثبيت والتحقق:
- **معرف الحزمة:** `com.krmzi.app` | **versionCode:** `41` | **SHA-256 للشهادة:** `2c6d74a5251577fcdfd8a9df651d52fb2641ff4ef1be6686c8266abed0a0cbf3`
- **حالة التحقق:** اجتاز البناء محاذاة 16KB وتوقيع APK Signature Scheme v2 واختبارات سلامة الأصول المدمجة.
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
    ]

    req = urllib.request.Request(f'https://api.github.com/repos/XRTM8/ZOET/releases/{release_info["id"]}', headers=headers)
    with urllib.request.urlopen(req) as resp:
        current_release = json.loads(resp.read().decode())
    existing_assets = {a['name']: a['id'] for a in current_release.get('assets', [])}

    # Delete deprecated desktop executable if present
    if 'ZOET.exe' in existing_assets:
        print(f"Removing deprecated ZOET.exe asset (ID: {existing_assets['ZOET.exe']})...")
        del_cmd = [
            'curl.exe', '--ssl-no-revoke', '-s', '-X', 'DELETE',
            '-H', f'Authorization: token {token}',
            f'https://api.github.com/repos/XRTM8/ZOET/releases/assets/{existing_assets["ZOET.exe"]}'
        ]
        subprocess.run(del_cmd, check=True)
        time.sleep(1)

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
