"""
DeepSeek API Balance — Quick Connection Test
============================================
Run this script to verify your API key works before launching the full app.

Usage:
  python test_api.py YOUR_API_KEY
"""

import sys
import requests

def test_balance(api_key: str):
    url = "https://api.deepseek.com/user/balance"
    headers = {
        "Accept": "application/json",
        "Authorization": f"Bearer {api_key}",
    }
    print(f"Querying: {url}")
    resp = requests.get(url, headers=headers, timeout=15)
    print(f"Status:   {resp.status_code}")

    if resp.status_code == 401:
        print("ERROR: Invalid API Key (401 Unauthorized)")
        return

    resp.raise_for_status()
    data = resp.json()
    infos = data.get("balance_infos", [])
    if not infos:
        print("WARNING: No balance info in response")
        return

    info = infos[0]
    print(f"Available:  {data.get('is_available')}")
    print(f"Currency:   {info.get('currency')}")
    print(f"Total:      ¥{float(info.get('total_balance', 0)):,.2f}")
    print(f"Topped-up:  ¥{float(info.get('topped_up_balance', 0)):,.2f}")
    print(f"Granted:    ¥{float(info.get('granted_balance', 0)):,.2f}")
    print("\nAPI key is valid. You're ready to use the tray app!")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python test_api.py YOUR_DEEPSEEK_API_KEY")
        sys.exit(1)
    test_balance(sys.argv[1])
