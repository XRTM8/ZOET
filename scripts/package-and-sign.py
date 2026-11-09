import os
import sys
import struct
import zlib
import hashlib
from androguard.core.apk import APK
from cryptography.hazmat.primitives.serialization import pkcs12, Encoding, PublicFormat
from cryptography.hazmat.primitives.asymmetric import padding
from cryptography.hazmat.primitives import hashes

def main():
    print("=== ZOET v2.8.0 APK Packager & Signer ===")
    
    base_apk_path = 'android/app/build/outputs/apk/release/app-release.apk'
    out_apk_path = 'release-output/app-release.apk'
    out_play_path = 'release-output/otium-play.apk'
    os.makedirs('release-output', exist_ok=True)
    
    with open(base_apk_path, 'rb') as f:
        orig_apk = f.read()

    # Parse central directory
    eocd_pos = orig_apk.rfind(b'PK\x05\x06')
    cd_offset = struct.unpack('<I', orig_apk[eocd_pos+16:eocd_pos+20])[0]
    cd_size = struct.unpack('<I', orig_apk[eocd_pos+12:eocd_pos+16])[0]
    num_entries = struct.unpack('<H', orig_apk[eocd_pos+8:eocd_pos+10])[0]

    entries = []
    pos = cd_offset
    while pos < cd_offset + cd_size:
        if orig_apk[pos:pos+4] != b'PK\x01\x02':
            break
        cm = struct.unpack('<H', orig_apk[pos+10:pos+12])[0]
        time = struct.unpack('<H', orig_apk[pos+12:pos+14])[0]
        date = struct.unpack('<H', orig_apk[pos+14:pos+16])[0]
        crc = struct.unpack('<I', orig_apk[pos+16:pos+20])[0]
        c_size = struct.unpack('<I', orig_apk[pos+20:pos+24])[0]
        u_size = struct.unpack('<I', orig_apk[pos+24:pos+28])[0]
        fn_len = struct.unpack('<H', orig_apk[pos+28:pos+30])[0]
        extra_len = struct.unpack('<H', orig_apk[pos+30:pos+32])[0]
        comment_len = struct.unpack('<H', orig_apk[pos+32:pos+34])[0]
        local_offset = struct.unpack('<I', orig_apk[pos+42:pos+46])[0]
        fn = orig_apk[pos+46:pos+46+fn_len].decode('utf-8', errors='replace')
        
        l_extra_len = struct.unpack('<H', orig_apk[local_offset+28:local_offset+30])[0]
        data_offset = local_offset + 30 + fn_len + l_extra_len
        raw_data = orig_apk[data_offset:data_offset+c_size]
        
        entries.append({
            'fn': fn,
            'cm': cm,
            'time': time,
            'date': date,
            'crc': crc,
            'c_size': c_size,
            'u_size': u_size,
            'local_offset': local_offset,
            'raw_data': raw_data,
            'fn_bytes': orig_apk[pos+46:pos+46+fn_len],
            'version_made_by': struct.unpack('<H', orig_apk[pos+4:pos+6])[0],
            'version_needed': struct.unpack('<H', orig_apk[pos+6:pos+8])[0],
            'flags': struct.unpack('<H', orig_apk[pos+8:pos+10])[0],
            'disk_start': struct.unpack('<H', orig_apk[pos+34:pos+36])[0],
            'int_attrs': struct.unpack('<H', orig_apk[pos+36:pos+38])[0],
            'ext_attrs': struct.unpack('<I', orig_apk[pos+38:pos+42])[0],
        })
        pos += 46 + fn_len + extra_len + comment_len

    entries.sort(key=lambda x: x['local_offset'])

    # Build new zip content
    new_apk_parts = []
    new_cd_parts = []
    current_offset = 0

    assets_root = 'android/app/src/main/assets/public'

    for e in entries:
        fn = e['fn']
        cm = e['cm']
        
        if fn == 'AndroidManifest.xml':
            # Patch manifest: uncompress original, update vname and vcode, recompress
            orig_manifest = zlib.decompress(e['raw_data'], -15) if cm == 8 else e['raw_data']
            m = bytearray(orig_manifest)
            # Patch versionName 2.8.0
            m[1264:1274] = '2.8.0'.encode('utf-16le')
            # Patch versionCode 26
            struct.pack_into('<I', m, 4180, 26)
            uncompressed = bytes(m)
            u_size = len(uncompressed)
            crc = zlib.crc32(uncompressed) & 0xffffffff
            
            # Compress with raw deflate (-15)
            comp_obj = zlib.compressobj(level=9, method=zlib.DEFLATED, wbits=-15)
            c_data = comp_obj.compress(uncompressed) + comp_obj.flush()
            c_size = len(c_data)
            cm = 8
            
        elif fn.startswith('assets/public/'):
            rel_path = fn[len('assets/public/'):]
            disk_path = os.path.join(assets_root, rel_path.replace('/', os.sep))
            if os.path.exists(disk_path):
                with open(disk_path, 'rb') as f_in:
                    uncompressed = f_in.read()
                u_size = len(uncompressed)
                crc = zlib.crc32(uncompressed) & 0xffffffff
                if cm == 0:
                    c_data = uncompressed
                    c_size = u_size
                else:
                    comp_obj = zlib.compressobj(level=9, method=zlib.DEFLATED, wbits=-15)
                    c_data = comp_obj.compress(uncompressed) + comp_obj.flush()
                    c_size = len(c_data)
            else:
                c_data = e['raw_data']
                c_size = e['c_size']
                u_size = e['u_size']
                crc = e['crc']
        else:
            # Keep original raw data
            c_data = e['raw_data']
            c_size = e['c_size']
            u_size = e['u_size']
            crc = e['crc']

        # Determine alignment
        fn_bytes = e['fn_bytes']
        lfh_len_without_extra = 30 + len(fn_bytes)
        extra = b''
        if cm == 0:
            align = 4096 if fn.endswith('.so') else 4
            target_data_offset = current_offset + lfh_len_without_extra
            pad = (align - (target_data_offset % align)) % align
            extra = b'\x00' * pad

        lfh = (
            b'PK\x03\x04' +
            struct.pack('<HHHHHIIIHH',
                e['version_needed'],
                e['flags'],
                cm,
                e['time'],
                e['date'],
                crc,
                c_size,
                u_size,
                len(fn_bytes),
                len(extra)
            ) +
            fn_bytes +
            extra
        )
        
        local_offset = current_offset
        new_apk_parts.append(lfh)
        new_apk_parts.append(c_data)
        current_offset += len(lfh) + len(c_data)
        
        # Central directory header (no extra padding in central directory)
        cd_record = (
            b'PK\x01\x02' +
            struct.pack('<HHHHHHIIIHHHHHII',
                e['version_made_by'],
                e['version_needed'],
                e['flags'],
                cm,
                e['time'],
                e['date'],
                crc,
                c_size,
                u_size,
                len(fn_bytes),
                0, # extra len
                0, # comment len
                e['disk_start'],
                e['int_attrs'],
                e['ext_attrs'],
                local_offset
            ) +
            fn_bytes
        )
        new_cd_parts.append(cd_record)

    cd_bytes = b''.join(new_cd_parts)
    cd_start = current_offset
    cd_len = len(cd_bytes)

    eocd = (
        b'PK\x05\x06' +
        struct.pack('<HHHHIIH',
            0, # disk num
            0, # cd disk
            len(entries), # entries on disk
            len(entries), # total entries
            cd_len,
            cd_start,
            0 # comment len
        )
    )

    unsigned_apk = b''.join(new_apk_parts) + cd_bytes + eocd
    print(f"Unsigned APK assembled: {len(unsigned_apk)} bytes")

    # Read signing properties and keystore
    props = {}
    with open('android/signing.properties') as f:
        for line in f:
            if '=' in line and not line.startswith('#'):
                k, v = line.strip().split('=', 1)
                props[k] = v

    with open('.signing/krmzi-release.keystore', 'rb') as f:
        p12 = f.read()

    priv, cert, _ = pkcs12.load_key_and_certificates(p12, props['storePassword'].encode())
    cert_der = cert.public_bytes(Encoding.DER)

    # APK Signature Scheme v2 calculation
    # Section 1: 0 to cd_start
    sec1 = unsigned_apk[:cd_start]
    # Section 2: Central Directory
    sec2 = unsigned_apk[cd_start:cd_start+cd_len]
    # Section 3: EoCD
    sec3 = unsigned_apk[cd_start+cd_len:]

    CHUNK = 1048576
    chunks = []
    for sec in [sec1, sec2, sec3]:
        for i in range(0, len(sec), CHUNK):
            c = sec[i:i+CHUNK]
            chunks.append(hashlib.sha256(b'\xa5' + struct.pack('<I', len(c)) + c).digest())

    content_digest = hashlib.sha256(b'\x5a' + struct.pack('<I', len(chunks)) + b''.join(chunks)).digest()

    digest_entry = struct.pack('<III', 40, 0x0103, 32) + content_digest
    digests_field = struct.pack('<I', len(digest_entry)) + digest_entry
    cert_entry = struct.pack('<I', len(cert_der)) + cert_der
    certs_field = struct.pack('<I', len(cert_entry)) + cert_entry
    attrs_field = struct.pack('<I', 0)

    signed_data_content = digests_field + certs_field + attrs_field
    signed_data = struct.pack('<I', len(signed_data_content)) + signed_data_content

    sig_bytes = priv.sign(signed_data, padding.PKCS1v15(), hashes.SHA256())
    sig_entry = struct.pack('<III', len(sig_bytes) + 8, 0x0103, len(sig_bytes)) + sig_bytes
    sigs_field = struct.pack('<I', len(sig_entry)) + sig_entry

    pub_der = priv.public_key().public_bytes(Encoding.DER, PublicFormat.SubjectPublicKeyInfo)
    pub_field = struct.pack('<I', len(pub_der)) + pub_der

    signer_content = signed_data + sigs_field + pub_field
    signer = struct.pack('<I', len(signer_content)) + signer_content
    signers_field = struct.pack('<I', len(signer)) + signer

    pair_val = signers_field
    pair = struct.pack('<QI', len(pair_val) + 4, 0x7109871a) + pair_val

    block_content_size = len(pair) + 24
    apk_signing_block = struct.pack('<Q', block_content_size) + pair + struct.pack('<Q', block_content_size) + b'APK Sig Block 42'

    new_sec3 = bytearray(sec3)
    struct.pack_into('<I', new_sec3, 16, len(sec1) + len(apk_signing_block))

    final_apk = sec1 + apk_signing_block + sec2 + bytes(new_sec3)

    with open(out_apk_path, 'wb') as f:
        f.write(final_apk)
    with open(out_play_path, 'wb') as f:
        f.write(final_apk)
    with open(base_apk_path, 'wb') as f:
        f.write(final_apk)

    print(f"Saved signed APK ({len(final_apk)} bytes) to:")
    print(f" - {out_apk_path}")
    print(f" - {out_play_path}")
    print(f" - {base_apk_path}")

    # Verify with Androguard
    a = APK(out_apk_path)
    pkg = a.get_package()
    vcode = a.get_androidversion_code()
    vname = a.get_androidversion_name()
    signed_v2 = a.is_signed_v2()
    certs = a.get_certificates_der_v2()
    cert_sha = hashlib.sha256(certs[0]).hexdigest()

    print("\n=== Androguard Verification ===")
    print(f"Package: {pkg}")
    print(f"versionCode: {vcode}")
    print(f"versionName: {vname}")
    print(f"is_signed_v2: {signed_v2}")
    print(f"Certificate SHA256: {cert_sha}")

    expected_sha = '2c6d74a5251577fcdfd8a9df651d52fb2641ff4ef1be6686c8266abed0a0cbf3'
    assert pkg == 'com.krmzi.app', f"Unexpected package {pkg}"
    assert vcode == '26', f"Unexpected versionCode {vcode}"
    assert vname == '2.8.0', f"Unexpected versionName {vname}"
    assert signed_v2 == True, "APK is not signed with v2"
    assert cert_sha == expected_sha, f"Cert SHA mismatch: {cert_sha} != {expected_sha}"
    
    print("\nALL VERIFICATIONS PASSED 100%! READY FOR RELEASE!")

if __name__ == '__main__':
    main()
