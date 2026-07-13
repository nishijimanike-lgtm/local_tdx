import sys
import io
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')
import argparse
import time
import json
import threading
from tdxrs.downloader import Downloader, ServerPool, TdxDirectClient

# Monkey patch ServerPool.__init__ to use a smaller timeout (e.g. 2.0s) for connections
original_init = ServerPool.__init__

def patched_init(self, servers=None, rate_limit=15, phase=None):
    original_init(self, servers, rate_limit, phase)
    self._clients = []
    for name, ip, port in self._servers:
        c = TdxDirectClient(ip, port, 2.0)  # Use 2.0s timeout
        self._clients.append((name, c))

ServerPool.__init__ = patched_init


def progress_worker(dl, stop_event):
    """Periodically fetches and prints progress in JSON format to stdout."""
    last_val = None
    while not stop_event.is_set():
        try:
            prog = dl.progress()
            # Avoid printing identical values sequentially
            if prog != last_val:
                print(f"PROGRESS:{json.dumps(prog)}", flush=True)
                last_val = prog
        except Exception as e:
            print(f"DEBUG: Failed to get progress: {e}", flush=True)
        time.sleep(0.5)

def main():
    parser = argparse.ArgumentParser(description="tdxrs data downloader bridge")
    parser.add_argument("--tdx-dir", required=True, help="Path to tdx data directory")
    parser.add_argument("--mode", choices=["full", "incremental"], default="incremental", help="Download mode")
    parser.add_argument("--rate-limit", type=int, default=15, help="RPS rate limit")
    parser.add_argument("--markets", default="sh,sz", help="Markets to download (comma separated)")
    
    args = parser.parse_args()
    markets = [m.strip().lower() for m in args.markets.split(",") if m.strip()]
    
    # Dynamic server latency probing to filter out offline/dead servers
    import socket
    from tdxrs.downloader import _DEFAULT_SERVERS
    
    print("INFO: Probing TDX servers for connectivity and latency...", flush=True)
    active_servers = []
    for name, ip, port in _DEFAULT_SERVERS:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(1.0)
            t0 = time.perf_counter()
            s.connect((ip, port))
            latency = (time.perf_counter() - t0) * 1000
            s.close()
            print(f"INFO: Server '{name}' ({ip}:{port}) is alive - {latency:.1f}ms", flush=True)
            active_servers.append((latency, (name, ip, port)))
        except Exception as e:
            print(f"INFO: Server '{name}' ({ip}:{port}) is offline or timed out ({e})", flush=True)
            
    if active_servers:
        active_servers.sort(key=lambda x: x[0])
        servers_list = [item[1] for item in active_servers]
        print(f"INFO: Selected {len(servers_list)} active servers sorted by latency.", flush=True)
    else:
        print("ERROR: No responsive TDX servers found! Falling back to default list.", flush=True)
        servers_list = _DEFAULT_SERVERS

    # Initialize tdxrs downloader
    # format='tdx' writes raw .day files to target dir
    dl = Downloader(
        data_dir=args.tdx_dir,
        rate_limit=args.rate_limit,
        servers=servers_list,
        format="tdx"
    )
    
    stop_event = threading.Event()
    progress_thread = threading.Thread(target=progress_worker, args=(dl, stop_event), daemon=True)
    progress_thread.start()
    
    success = False
    error_msg = ""
    try:
        if args.mode == "full":
            print(f"INFO: Starting full download for markets: {markets}...", flush=True)
            dl.run(markets=markets, categories=["daily"])
        else:
            print(f"INFO: Starting incremental update for markets: {markets}...", flush=True)
            dl.update(markets=markets, categories=["daily"])
        success = True
    except Exception as e:
        error_msg = str(e)
        print(f"ERROR: Download process encountered error: {e}", flush=True)
    finally:
        # Signal progress thread to stop
        stop_event.set()
        progress_thread.join(timeout=1.0)
        
        # Print final result summary
        final_prog = dl.progress()
        result = {
            "success": success,
            "error": error_msg,
            "done": final_prog.get("done", 0),
            "skipped": final_prog.get("skipped", 0),
            "failed": final_prog.get("failed", 0),
            "total_errors": len(final_prog.get("errors", []))
        }
        print(f"COMPLETED:{json.dumps(result)}", flush=True)
        
    if not success:
        sys.exit(1)

if __name__ == "__main__":
    main()
