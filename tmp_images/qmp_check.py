import json, socket, sys, time

port = int(sys.argv[1])
ppm = sys.argv[2]
deadline = time.time() + 20
last_err = None
while time.time() < deadline:
    try:
        s = socket.create_connection(("127.0.0.1", port), timeout=2)
        break
    except OSError as e:
        last_err = e
        time.sleep(0.5)
else:
    raise SystemExit(f"qmp connect failed: {last_err}")

f = s.makefile("rw", encoding="utf-8", newline="\n")
print("banner:", f.readline().strip())
f.write('{"execute":"qmp_capabilities"}\n')
f.flush()
print("caps:", f.readline().strip())
f.write('{"execute":"query-status"}\n')
f.flush()
status = f.readline().strip()
print("status:", status)
cmd = {"execute": "screendump", "arguments": {"filename": ppm}}
f.write(json.dumps(cmd) + "\n")
f.flush()
print("dump:", f.readline().strip())
s.close()

with open(ppm, "rb") as fh:
    data = fh.read()
assert data.startswith(b"P6"), data[:20]
# Skip PPM header (P6\nW H\n255\n)
idx = 0
nl = 0
while nl < 3:
    if data[idx] == 10:
        nl += 1
    idx += 1
pixels = data[idx:]
nonblack = sum(1 for i in range(0, len(pixels), 3) if pixels[i] | pixels[i + 1] | pixels[i + 2])
print(f"ppm_bytes={len(data)} nonblack={nonblack}")
if "running" not in status:
    raise SystemExit(2)
if nonblack < 1000:
    raise SystemExit(3)
print("PASS")
