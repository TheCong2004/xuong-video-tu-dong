from pydantic import BaseModel, Field


class CreateDraftRequest(BaseModel):
    height: int = Field(default=1080, ge=1, description="Video height")
    width: int = Field(default=1920, ge=1, description="Video width")


class CreateDraftResponse(BaseModel):
    draft_url: str = Field(default="", description="Draft URL")
    draft_id: str = Field(default="", description="Canonical draft identifier")
    draft_path: str = Field(default="", description="Canonical backend-owned draft directory")
    tip_url: str = Field(default="", description="Documentation URL")
