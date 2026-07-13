import sys
import io
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')
import os
import sqlite3
import pandas as pd
from pytdx.reader import GbbqReader

def main():
    if len(sys.argv) < 3:
        print("Usage: parse_gbbq.py <db_path> <tdx_dir>")
        sys.exit(1)
        
    db_path = sys.argv[1]
    tdx_dir = sys.argv[2]
    
    gbbq_path = os.path.join(tdx_dir, "T0002", "hq_cache", "gbbq")
    if not os.path.exists(gbbq_path):
        print(f"Error: gbbq file not found at {gbbq_path}")
        sys.exit(1)
        
    print(f"Loading GBBQ from {gbbq_path}...")
    reader = GbbqReader()
    df = reader.get_df(gbbq_path)
    
    # Filter category = 1 (除权除息) and 14 (股改权息)
    df = df[df['category'].isin([1, 14])].copy()
    
    # Map market and code to symbol and market code
    # market in tdx: 1 = SH, 0 = SZ (which can include BJ)
    BJ_A_SHARE_PREFIXES = ('83', '87', '88', '43')
    
    records = []
    for _, row in df.iterrows():
        market_val = row['market']
        code_val = str(row['code']).zfill(6)
        
        # Resolve market type: Sz = 0, Sh = 1, Bj = 2
        market_type = None
        if market_val == 1:
            market_type = 1 # Market::Sh is 1
        elif market_val == 0:
            if code_val.startswith(BJ_A_SHARE_PREFIXES):
                market_type = 2 # Market::Bj is 2
            else:
                market_type = 0 # Market::Sz is 0
                
        if market_type is None:
            continue
            
        # Parse datetime
        dt_val = str(row['datetime'])
        if len(dt_val) == 8:
            ex_date = f"{dt_val[:4]}-{dt_val[4:6]}-{dt_val[6:]}"
        else:
            continue
            
        # Scale values (per 10 shares in TDX -> per 1 share)
        fenhong = float(row['hongli_panqianliutong']) / 10.0
        peigujia = float(row['peigujia_qianzongguben'])
        peigu = float(row['peigu_houzongguben']) / 10.0
        # songgu_qianzongguben is songzhuangu
        songzhuangu = float(row['songgu_qianzongguben']) / 10.0
        
        records.append((
            market_type,
            code_val,
            ex_date,
            int(row['category']),
            fenhong,
            peigu,
            peigujia,
            songzhuangu,
            "local_gbbq",
            pd.Timestamp.now().strftime("%Y-%m-%d %H:%M:%S")
        ))
        
    print(f"Parsed {len(records)} XDXR events. Writing to DB...")
    
    conn = sqlite3.connect(db_path)
    cur = conn.cursor()
    
    # Upsert into xdxr_events
    cur.executemany("""
        INSERT INTO xdxr_events (market, symbol, ex_date, category, fenhong, peigu, peigujia, songzhuangu, source, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(market, symbol, ex_date, category) DO UPDATE SET
            fenhong=excluded.fenhong, peigu=excluded.peigu, peigujia=excluded.peigujia,
            songzhuangu=excluded.songzhuangu, updated_at=excluded.updated_at
    """, records)
    
    conn.commit()
    conn.close()
    print("XDXR Sync successful.")

if __name__ == '__main__':
    main()
