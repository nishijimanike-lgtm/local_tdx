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

state = {
    "paused": False,
    "aborted": False
}

def stdin_listener(stop_event):
    """Background listener for sys.stdin control commands (PAUSE, RESUME, ABORT)."""
    while not stop_event.is_set():
        try:
            line = sys.stdin.readline()
            if not line:
                break
            cmd = line.strip().upper()
            if cmd == "PAUSE":
                state["paused"] = True
                print("INFO: Task paused.", flush=True)
            elif cmd == "RESUME":
                state["paused"] = False
                print("INFO: Task resumed.", flush=True)
            elif cmd == "ABORT":
                state["aborted"] = True
                print("INFO: Task aborted.", flush=True)
        except Exception:
            break

def progress_worker(dl, stop_event):
    """Periodically fetches and prints progress in JSON format to stdout."""
    last_val = None
    while not stop_event.is_set():
        try:
            prog = dl.progress()
            # Inject paused status into progress json
            prog["paused"] = state["paused"]
            prog["aborted"] = state["aborted"]
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
    
    # Start stdin listener thread
    stdin_stop = threading.Event()
    stdin_thread = threading.Thread(target=stdin_listener, args=(stdin_stop,), daemon=True)
    stdin_thread.start()
    
    success = False
    error_msg = ""
    try:
        from tdxrs.constants import MARKET_SH, MARKET_SZ, MARKET_BJ
        from tdxrs.downloader import _CATEGORY_MAP
        
        market_map = {"sh": MARKET_SH, "sz": MARKET_SZ, "bj": MARKET_BJ}
        categories = ["daily"]
        
        # 1. Determine stock list
        stock_list_by_market = {}
        
        if args.mode == "incremental":
            sync_data = dl._load_sync()
            dl._sync_data = sync_data
            if not sync_data:
                print("INFO: No historical sync data found. Executing full download.", flush=True)
                args.mode = "full"
            else:
                for key in sync_data:
                    parts = key.split("/")
                    if len(parts) == 2:
                        m_name, code = parts
                        if m_name in markets:
                            if m_name not in stock_list_by_market:
                                stock_list_by_market[m_name] = []
                            stock_list_by_market[m_name].append(code)
                            
        if args.mode == "full":
            for m_name in markets:
                mkt = market_map.get(m_name)
                if mkt is not None:
                    print(f"INFO: Fetching stock list for market '{m_name}'...", flush=True)
                    stock_list_by_market[m_name] = [item[1] for item in dl._fetch_stock_list(mkt)]
                    
        # 2. Start download loop
        aborted = False
        for cat_name in categories:
            if cat_name not in _CATEGORY_MAP:
                continue
            cat_code, dir_name, max_per_req = _CATEGORY_MAP[cat_name]
            is_minute = cat_code < 4
            
            for m_name in markets:
                mkt = market_map.get(m_name)
                if mkt is None:
                    continue
                codes = stock_list_by_market.get(m_name, [])
                total = len(codes)
                if total == 0:
                    continue
                
                print(f"INFO: Starting download loop for {m_name}/{dir_name}: {total} stocks...", flush=True)
                
                for i, code in enumerate(codes):
                    # Check pause
                    if state["paused"]:
                        while state["paused"] and not state["aborted"]:
                            time.sleep(0.2)
                    
                    # Check abort
                    if state["aborted"]:
                        aborted = True
                        break
                        
                    try:
                        n = dl._download_one(mkt, code, cat_code, dir_name, max_per_req, is_minute)
                        dl._stats["done"] += 1
                        if n > 0:
                            print(f"  [{i+1}/{total}] {code}: +{n} 条", flush=True)
                        else:
                            dl._stats["skipped"] += 1
                    except Exception as e:
                        dl._stats["failed"] += 1
                        dl._stats["errors"].append(f"{code}: {e}")
                        print(f"  [{i+1}/{total}] {code}: ERROR {e}", flush=True)
                        
                    if (i + 1) % 50 == 0:
                        dl._save_checkpoint(m_name, dir_name, code, i + 1, total)
                
                if aborted:
                    break
                dl._save_checkpoint(m_name, dir_name, "", total, total)
            
            if aborted:
                break
        
        # Save final sync data
        dl._print_summary()
        success = not aborted
        if aborted:
            error_msg = "Task aborted by user."
            
    except Exception as e:
        error_msg = str(e)
        print(f"ERROR: Download process encountered error: {e}", flush=True)
    finally:
        # Signal progress and stdin threads to stop
        stop_event.set()
        progress_thread.join(timeout=1.0)
        
        stdin_stop.set()
        
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
