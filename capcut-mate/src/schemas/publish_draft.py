from typing import Dict

from pydantic import BaseModel, Field


class PublishDraftRequest(BaseModel):
    draft_id: str = Field(..., min_length=1)
    staging_path: str = Field(..., min_length=1)


class PublishDraftResponse(BaseModel):
    draft_id: str
    staging_path: str
    desktop_root: str
    final_path: str
    media: Dict[str, int]
