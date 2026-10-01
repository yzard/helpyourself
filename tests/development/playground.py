import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("playground", ROOT / "src/development/playground.py")
playground = importlib.util.module_from_spec(spec)
spec.loader.exec_module(playground)


class PlaygroundTests(unittest.TestCase):
    def test_filters_container_loopback_and_down_interfaces(self):
        def interface(name, address, state):
            return {
                "ifname": name,
                "operstate": state,
                "addr_info": [{"family": "inet", "scope": "global", "local": address}],
            }

        records = [
            interface("en0", "192.168.1.4", "UP"),
            interface("docker0", "172.17.0.1", "UP"),
            interface("br-test", "172.18.0.1", "UP"),
            interface("lo", "127.0.0.1", "UP"),
            interface("eth1", "10.1.0.2", "DOWN"),
        ]
        self.assertEqual(playground.lan_addresses(records), ["192.168.1.4"])

    def test_ports_come_from_compose_and_fallback_is_explicit(self):
        config = {
            "services": {
                "caddy": {
                    "ports": [{"target": 443, "published": "24443"}],
                    "environment": {"HELPYOURSELF_DOMAIN": "health.test"},
                }
            }
        }
        output = playground.describe(config, [])
        self.assertIn("https://health.test:24443", output)
        self.assertIn("0.0.0.0:24443 -> caddy:443", output)
        self.assertNotIn("https://0.0.0.0", output)
        self.assertIn("discovery unavailable", output)


if __name__ == "__main__":
    unittest.main()
