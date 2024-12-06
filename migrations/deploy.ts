// Migrations are an early feature. Currently, they're nothing more than this
// single deploy script that's invoked from the CLI, injecting a provider
// configured from the workspace's Anchor.toml.

import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { BonkArena } from "../target/types/bonk_arena";
import * as dotenv from "dotenv";

dotenv.config();

export async function deployProgram() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.BonkArena as Program<BonkArena>;

  // 从环境变量读取密钥
  const gameSecretKey = process.env.GAME_SECRET_KEY;
  if (!gameSecretKey) {
    throw new Error("GAME_SECRET_KEY not found in environment variables");
  }

  // 将字符串转换为 Uint8Array (32 bytes)
  const secretKeyBytes = new TextEncoder().encode(gameSecretKey);
  const secretKeyArray = new Uint8Array(32);
  secretKeyArray.set(secretKeyBytes.slice(0, 32));

  try {
    // 初始化程序
    await program.methods
      .initialize(
        new anchor.BN(1000000), // entry_fee
        70, // prize_ratio
        30, // commission_ratio
        [50, 30, 20], // prize_distribution
      )
      .accounts({
        // ... 账户配置
      })
      .rpc();

    // 设置密钥
    await program.methods
      .setSecretKey(Array.from(secretKeyArray))
      .accounts({
        leaderboard: program.programId,
      })
      .rpc();

    console.log("Program deployed successfully with secret key");
  } catch (error) {
    console.error("Deployment failed:", error);
    throw error;
  }
}

// 如果直接运行此脚本
if (require.main === module) {
  deployProgram().then(
    () => process.exit(0),
    (err) => {
      console.error(err);
      process.exit(1);
    }
  );
}
