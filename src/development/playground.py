"""Describe the resolved Compose ingress without exposing credentials or private config."""

import json
import subprocess
import sys


def lan_addresses(interfaces):
    addresses = []
    for interface in interfaces:
        name = interface.get("ifname", "")
        if name.startswith(("lo", "docker", "br-", "veth", "virbr")):
            continue
        if interface.get("operstate") != "UP":
            continue
        for address in interface.get("addr_info", []):
            if address.get("family") == "inet" and address.get("scope") == "global":
                addresses.append(address["local"])
    return sorted(set(addresses))


def describe(configuration, addresses):
    caddy = configuration["services"]["caddy"]
    secure_port = next(port for port in caddy["ports"] if int(port["target"]) == 443)
    host_port = secure_port["published"]
    bind = secure_port.get("host_ip", "0.0.0.0")
    domain = caddy["environment"]["HELPYOURSELF_DOMAIN"]
    lines = [
        f"HTTPS ingress: {bind}:{host_port} -> caddy:443 -> backend_api:8080 (HTTP)",
        f"Server URL: https://{domain}:{host_port}",
    ]
    for address in addresses:
        lines.append(
            f"LAN route: {address}:{host_port}; use the configured hostname and a matching trusted TLS certificate."
        )
    if not addresses:
        lines.append(
            "LAN address discovery unavailable; use the host's network settings. No public IP lookup was performed."
        )
    lines.append("Foreground logs follow. Ctrl+C stops this playground without deleting its data.")
    return "\n".join(lines)


def main():
    configuration = json.load(sys.stdin)
    try:
        result = subprocess.run(["ip", "-json", "address", "show"], check=True, capture_output=True, text=True)
        addresses = lan_addresses(json.loads(result.stdout))
    except (OSError, subprocess.CalledProcessError, json.JSONDecodeError):
        addresses = []
    print(describe(configuration, addresses))


if __name__ == "__main__":
    main()
