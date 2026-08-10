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

with open(ppm, "rb") as pf:
    data = pf.read()
# PPM P6 header then RGB triples
nl = data.find(b"\n", data.find(b"\n", data.find(b"\n") + 1) + 1) + 1
pixels = data[nl:]
non_black = sum(1 for i in range(0, len(pixels), 3) if pixels[i] | pixels[i + 1] | pixels[i + 2])
print(f"ppm_bytes={len(data)} non_black={non_black}")
if non_black < 1000:
    raise SystemExit("screen too black — likely hang/black-screen")
if '"status": "running"' not in status and '"status":"running"' not in status:
    # tolerate whitespace variants
    if "running" not in status:
        raise SystemExit(f"not running: {status}")
print("PASS")
