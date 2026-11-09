import os
import sys
import json
import urllib.request
import subprocess
import time

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
    
    headers = {
        'Authorization': 'token ' + token,
        'Accept': 'application/vnd.github.v3+json',
        'User-Agent': 'ZOET-Release-Uploader'
    }

    # Check if release exists
    check_req = urllib.request.Request(f'https://api.github.com/repos/XRTM8/ZOET/releases/tags/{tag_name}', headers=headers)
    with urllib.request.urlopen(check_req) as resp:
        release_info = json.loads(resp.read().decode())
    
    print(f"Release found: ID {release_info['id']}")

    upload_url_template = release_info['upload_url']
    upload_url_base = upload_url_template.split('{')[0]

    # Files to upload
    files_to_upload = [
        ('app-release.apk', 'release-output/app-release.apk', 'application/vnd.android.package-archive'),
        ('otium-play.apk', 'release-output/otium-play.apk', 'application/vnd.android.package-archive'),
        ('ZOET.exe', 'release-output/ZOET.exe', 'application/vnd.microsoft.portable-executable'),
    ]

    # Refresh release info to get current assets
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
            print(f"Deleted old asset {asset_name}")
            time.sleep(1)

        file_size = os.path.getsize(asset_path)
        print(f"\nUploading {asset_name} ({file_size} bytes) via curl...")
        upload_url = f"{upload_url_base}?name={asset_name}"
        
        # Use curl with retry, ssl-no-revoke and progress
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
        
        try:
            resp_data = json.loads(proc.stdout)
            print(f"Uploaded {asset_name} successfully! Download URL: {resp_data.get('browser_download_url')}")
        except Exception as e:
            print(f"Raw response: {proc.stdout[:300]}")

    print("\n=== RELEASE v2.8.0 PUBLISHED AND ASSETS UPLOADED SUCCESSFULLY! ===")
    print(f"Release URL: {release_info['html_url']}")

if __name__ == '__main__':
    main()
