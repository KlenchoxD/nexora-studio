import Shell from "./components/Shell";
import Conversation from "./pages/Conversation";
import UpdateBanner from "./components/UpdateBanner";

export default function App() {
  return (
    <>
      <UpdateBanner />
      <Shell active="conversation" onNav={() => {}}>
        <Conversation />
      </Shell>
    </>
  );
}
