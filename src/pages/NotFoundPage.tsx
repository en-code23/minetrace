import { MapPinned } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button } from "../components/ui/Button";
import { EmptyState } from "../components/ui/EmptyState";

export function NotFoundPage() {
  const navigate = useNavigate();
  return (
    <div className="page page--empty">
      <EmptyState
        icon={MapPinned}
        title="This trail ends here"
        description="The requested view does not exist in this MineTrace archive."
        action={<Button variant="primary" onClick={() => navigate("/overview")}>Return to overview</Button>}
      />
    </div>
  );
}
