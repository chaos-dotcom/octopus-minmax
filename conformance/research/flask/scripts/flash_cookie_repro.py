#!/usr/bin/env python3
"""Reproduce the Flask session/flash cookie byte-for-byte, stdlib only.

Verified against the raw captures in ../captures (see assert_captures()).
Target: Flask 3.1.0 + itsdangerous 2.2.0, app.secret_key = 'octobot-tool'.
"""
import base64, hashlib, hmac, json, re, struct, sys, zlib, pathlib

SECRET_KEY = b"octobot-tool"          # app.secret_key = 'octobot-tool'
SALT = b"cookie-session"              # SecureCookieSessionInterface.salt
SEP = b"."


def derive_key(secret_key=SECRET_KEY, salt=SALT):
    """Signer.derive_key() with key_derivation='hmac', digest_method=sha1."""
    mac = hmac.new(secret_key, digestmod=hashlib.sha1)
    mac.update(salt)
    return mac.digest()


def b64url(data: bytes) -> bytes:
    """itsdangerous.encoding.base64_encode: url-safe base64, '=' stripped."""
    return base64.urlsafe_b64encode(data).rstrip(b"=")


def json_payload(session_dict) -> bytes:
    """TaggedJSONSerializer.dumps() -> compact ASCII JSON bytes.

    flask.json.dumps(tagged, separators=(",", ":")) with
    DefaultJSONProvider defaults ensure_ascii=True and sort_keys=True.
    """
    return json.dumps(session_dict, separators=(",", ":"),
                      ensure_ascii=True, sort_keys=True).encode("utf-8")


def int_to_bytes(num: int) -> bytes:
    """itsdangerous.encoding.int_to_bytes: big-endian, leading NULs removed."""
    return struct.pack(">Q", num).lstrip(b"\x00")


def ts_bytes(ts: int) -> bytes:
    return b64url(int_to_bytes(ts))


def sign(value: bytes) -> bytes:
    """Signer.sign / HMACAlgorithm: HMAC-SHA1 over the value, base64url, no pad."""
    return b64url(hmac.new(derive_key(), value, hashlib.sha1).digest())


def signed_raw(json_bytes: bytes, timestamp: int, secret_key=SECRET_KEY) -> str:
    """Sign arbitrary bytes as the payload (used to test bad-payload handling)."""
    payload = b64url(json_bytes)
    value = payload + SEP + ts_bytes(timestamp)
    sig = b64url(hmac.new(derive_key(secret_key, SALT), value, hashlib.sha1).digest())
    return (value + SEP + sig).decode("ascii")


def cookie_value(session_dict, timestamp: int, secret_key=SECRET_KEY) -> str:
    """URLSafeTimedSerializer.dumps(dict(session)) as Flask does it."""
    json_bytes = json_payload(session_dict)
    compressed = zlib.compress(json_bytes)          # zlib default level
    if len(compressed) < (len(json_bytes) - 1):     # URLSafeSerializerMixin
        payload = b"." + b64url(compressed)
    else:
        payload = b64url(json_bytes)
    value = payload + SEP + ts_bytes(timestamp)
    sig = b64url(hmac.new(derive_key(secret_key, SALT), value, hashlib.sha1).digest())
    return (value + SEP + sig).decode("ascii")


def flash_cookie(messages, timestamp: int) -> str:
    """messages: [[category, text], ...] -> the session cookie value."""
    tagged = {"_flashes": [{" t": list(m)} for m in messages]}
    return cookie_value(tagged, timestamp)


def delete_cookie_header(value=""):
    """Werkzeug dump_cookie order: Domain, Expires, Max-Age, Secure, HttpOnly,
    Path, SameSite, Partitioned."""
    return ("session=%s; Expires=Thu, 01 Jan 1970 00:00:00 GMT; "
            "Max-Age=0; HttpOnly; Path=/" % value)


def set_cookie_header(value):
    return "session=%s; HttpOnly; Path=/" % value


# --------------------------------------------------------------------------
def cookie_from_capture(path):
    raw = pathlib.Path(path).read_bytes()
    m = re.search(rb"Set-Cookie: (session=[^;]*?); HttpOnly; Path=/", raw)
    return m.group(1).decode("ascii").split("=", 1)[1]


def decode(cookie):
    """Return (payload_b64, timestamp, signature, json_text_or_None)."""
    payload, ts, sig = cookie.rsplit(".", 2)
    if payload.startswith("."):
        body = b64pad_re(payload[1:])
        json_text = zlib.decompress(base64.urlsafe_b64decode(body)).decode("utf-8")
        compressed = True
    else:
        json_text = base64.urlsafe_b64decode(b64pad_re(payload)).decode("utf-8")
        compressed = False
    ts_int = struct.unpack(">Q", base64.urlsafe_b64decode(b64pad_re(ts)).rjust(8, b"\x00"))[0]
    return compressed, ts_int, sig, json_text


def b64pad_re(s):
    s = s.encode() if isinstance(s, str) else s
    return s + b"=" * (-len(s) % 4)


def main():
    cap = pathlib.Path(__file__).resolve().parent.parent / "captures"
    cases = [
        ("A25_post-config-valid", [["success", "Configuration updated successfully! (Will reset on container restart)"]]),
        ("A27_post-config-bad-time", [["error", "Execution time must be in HH:MM format (00:00 to 23:59)"]]),
        ("A29_post-config-bad-num", [["error", "Switch threshold must be a number"]]),
        ("A30_post-config-negative", [["error", "Switch threshold must be positive"]]),
        ("A31_post-config-both-bad", [["error", "Execution time must be in HH:MM format (00:00 to 23:59)"],
                                      ["error", "Switch threshold must be a number"]]),
    ]
    ok = True
    for name, messages in cases:
        got = cookie_from_capture(cap / (name + ".raw"))
        compressed, ts_int, sig, json_text = decode(got)
        mine = flash_cookie(messages, ts_int)
        same = (mine == got)
        ok &= same
        print("%-30s ts=%d compressed=%-5s match=%s" % (name, ts_int, compressed, same))
        print("    wire json        : %s" % json_text)
        if not same:
            print("    captured         : %s" % got)
            print("    recomputed       : %s" % mine)
    print()
    print("ALL CAPTURES REPRODUCED" if ok else "MISMATCH")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
