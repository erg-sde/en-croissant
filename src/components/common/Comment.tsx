import { Typography } from "@mantine/core";
import { memo } from "react";

import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { remarkUnderline } from "@/utils/remarkUnderline";

// Comments come from the user and from imported PGN files, so they are rendered as Markdown only:
// raw HTML in a comment is never turned into elements.
function Comment({ comment }: { comment: string }) {
  const multipleLine = comment.split("\n").filter((v) => v.trim() !== "").length > 1;

  return (
    <Typography
      pl={0}
      mx={4}
      style={{
        display: multipleLine ? "block" : "inline",
      }}
    >
      <Markdown
        components={{
          a: ({ node: _node, ...props }) => <a {...props} target="_blank" rel="noreferrer" />,
          p: ({ node: _node, ...props }) => (multipleLine ? <p {...props} /> : <span {...props} />),
        }}
        remarkPlugins={[remarkGfm, remarkUnderline]}
      >
        {comment}
      </Markdown>
    </Typography>
  );
}

export default memo(Comment);
