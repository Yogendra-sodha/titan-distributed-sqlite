from titan_db import TitanClient
import time

db = TitanClient(["http://127.0.0.1:8001", "http://127.0.0.1:8002", "http://127.0.0.1:8003"])
print(db.status())
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
print("Success")
