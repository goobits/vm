#!/usr/bin/env python3
"""Simple HTTP server for testing network connectivity between containers"""

from http.server import HTTPServer, BaseHTTPRequestHandler
import json
import socket
from datetime import datetime

class TestHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        response = {
            "status": "ok",
            "message": "Hello from the test server!",
            "server_hostname": socket.gethostname(),
            "server_ip": socket.gethostbyname(socket.gethostname()),
            "client_ip": self.client_address[0],
            "timestamp": datetime.now().isoformat(),
            "path": self.path
        }

        self.send_response(200)
        self.send_header('Content-type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(response, indent=2).encode())

        # Log the request
        print(f"[{datetime.now().isoformat()}] Request from {self.client_address[0]} - {self.path}")

    def log_message(self, format, *args):
        # Suppress default logging
        pass

if __name__ == '__main__':
    port = 8080
    server = HTTPServer(('0.0.0.0', port), TestHandler)
    print(f"🚀 Test server running on port {port}")
    print(f"   Hostname: {socket.gethostname()}")
    print(f"   IP: {socket.gethostbyname(socket.gethostname())}")
    print(f"\n📡 Other containers on 'jules' network can test with:")
    print(f"   curl http://{socket.gethostname()}:{port}")
    print(f"\n   Press Ctrl+C to stop\n")
    server.serve_forever()
