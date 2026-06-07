import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Login from "./Login";
import { checkAuth } from "./api";

function Root() {
  const [authed, setAuthed] = useState<boolean | null>(null);

  useEffect(() => {
    void checkAuth().then(setAuthed);
  }, []);

  if (authed === null) {
    return null;
  }

  if (!authed) {
    return <Login onSuccess={() => setAuthed(true)} />;
  }

  return <App onLogout={() => setAuthed(false)} />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
