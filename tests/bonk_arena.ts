import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { BonkArena } from "../target/types/bonk_arena";
import { TestToken } from "../target/types/test_token";
import { PublicKey, Keypair, SystemProgram, SYSVAR_RENT_PUBKEY } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, createAssociatedTokenAccount, getAssociatedTokenAddress } from "@solana/spl-token";
import { expect } from 'chai';

describe("bonk_arena", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const gameProgram = anchor.workspace.SolanaGameLeaderboard as Program<SolanaGameLeaderboard>;
  const tokenProgram = anchor.workspace.TestToken as Program<TestToken>;

  // 测试代币相关
  const mintKeypair = Keypair.generate();
  const authority = provider.wallet;
  let tokenAccount: PublicKey;

  // 游戏合约相关
  const leaderboardKeypair = new Keypair();
  let tokenPool: PublicKey;
  let gameSession: PublicKey;

  before(async () => {
    try {
      // 初始化测试代币
      await tokenProgram.methods
        .initialize()
        .accounts({
          mint: mintKeypair.publicKey,
          authority: authority.publicKey,
          payer: authority.publicKey,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          rent: SYSVAR_RENT_PUBKEY,
        })
        .signers([mintKeypair])
        .rpc();

      // 创建代币账户
      tokenAccount = await getAssociatedTokenAddress(
        mintKeypair.publicKey,
        authority.publicKey
      );

      await createAssociatedTokenAccount(
        provider.connection,
        provider.wallet.payer,
        mintKeypair.publicKey,
        authority.publicKey
      );

      // 铸造测试代币
      const amount = new anchor.BN(1000_000_000_000); // 1000 tokens with 9 decimals
      await tokenProgram.methods
        .mintTokens(amount)
        .accounts({
          mint: mintKeypair.publicKey,
          tokenAccount: tokenAccount,
          authority: authority.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      // 创建游戏代币池账户
      tokenPool = await getAssociatedTokenAddress(
        mintKeypair.publicKey,
        leaderboardKeypair.publicKey,
        true // allowOwnerOffCurve = true for PDA
      );

      await createAssociatedTokenAccount(
        provider.connection,
        provider.wallet.payer,
        mintKeypair.publicKey,
        leaderboardKeypair.publicKey
      );

      // 初始化游戏合约
      await gameProgram.methods
        .initialize(
          new anchor.BN(1_000_000_000), // 1 token entry fee
          70, // 70% prize ratio
          30, // 30% commission ratio
          [50, 30, 20] // prize distribution
        )
        .accounts({
          leaderboard: leaderboardKeypair.publicKey,
          payer: authority.publicKey,
          tokenMint: mintKeypair.publicKey,
          tokenPool: tokenPool,
          ownerTokenAccount: tokenAccount,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          rent: SYSVAR_RENT_PUBKEY,
        })
        .signers([leaderboardKeypair])
        .rpc();

    } catch (error) {
      console.error("Setup error:", error);
      throw error;
    }
  });

  it("Set secret key", async () => {
    try {
      const secretKey = new Uint8Array(32).fill(1); // 示例密钥
      await gameProgram.methods
        .setSecretKey(Array.from(secretKey))
        .accounts({
          leaderboard: leaderboardKeypair.publicKey,
        })
        .rpc();
    } catch (error) {
      console.error("Set secret key error:", error);
      throw error;
    }
  });

  it("Start game", async () => {
    try {
      const [gameSessionPda] = await PublicKey.findProgramAddress(
        [
          Buffer.from("player_session"),
          authority.publicKey.toBuffer(),
        ],
        gameProgram.programId
      );

      // 获取玩家的代币账户
      const payerTokenAccount = await getAssociatedTokenAddress(
        mintKeypair.publicKey,
        authority.publicKey
      );

      await gameProgram.methods
        .startGame("Player1")
        .accounts({
          leaderboard: leaderboardKeypair.publicKey,
          gameSession: gameSessionPda,
          payerTokenAccount: payerTokenAccount,
          tokenPool: tokenPool,
          tokenMint: mintKeypair.publicKey,
          payer: authority.publicKey,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();
    } catch (error) {
      console.error("Start game error:", error);
      throw error;
    }
  });

  it("Log score", async () => {
    try {
      const [gameSessionPda] = await PublicKey.findProgramAddress(
        [
          Buffer.from("player_session"),
          authority.publicKey.toBuffer(),
        ],
        gameProgram.programId
      );

      const fakeGameKey = new Uint8Array(32).fill(0);

      await gameProgram.methods
        .logScore(100, Array.from(fakeGameKey))
        .accounts({
          leaderboard: leaderboardKeypair.publicKey,
          gameSession: gameSessionPda,
          payer: authority.publicKey,
        })
        .rpc();
    } catch (error) {
      console.log("Expected error due to invalid game key:", error);
    }
  });

  it("End game", async () => {
    try {
      await gameProgram.methods
        .endGame()
        .accounts({
          leaderboard: leaderboardKeypair.publicKey,
          tokenPool: tokenPool,
          ownerTokenAccount: tokenAccount,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();
    } catch (error) {
      console.error("End game error:", error);
      throw error;
    }
  });

  it("Get leaderboard", async () => {
    try {
      // 获取排行榜数据
      const leaderboardAccount = await gameProgram.account.leaderboard.fetch(
        leaderboardKeypair.publicKey
      );

      console.log("Current Leaderboard:");
      console.log("Players:", leaderboardAccount.players);
      console.log("Prize Pool:", leaderboardAccount.prizePool?.toString());
      console.log("Commission Pool:", leaderboardAccount.commissionPool?.toString());

      // 验证排行榜数据
      expect(Array.isArray(leaderboardAccount.players)).to.be.true;

      // 如果有玩家记录，验证玩家数据结构
      if (leaderboardAccount.players.length > 0) {
        const player = leaderboardAccount.players[0];
        expect(player).to.have.property('address');
        expect(player).to.have.property('score');
        expect(player).to.have.property('name');
      }

      // 验证排行榜是否按分数排序
      const scores = leaderboardAccount.players.map(p => p.score);
      const sortedScores = [...scores].sort((a, b) => b - a);
      expect(scores).to.deep.equal(sortedScores);

      // 验证排行榜最多只有10名玩家
      expect(leaderboardAccount.players.length).to.be.lte(10);

    } catch (error) {
      console.error("Get leaderboard error:", error);
      throw error;
    }
  });

  it("Complete game flow with leaderboard update", async () => {
    try {
      // 1. 开始游戏
      const [gameSessionPda] = await PublicKey.findProgramAddress(
        [
          Buffer.from("player_session"),
          authority.publicKey.toBuffer(),
        ],
        gameProgram.programId
      );

      const payerTokenAccount = await getAssociatedTokenAddress(
        mintKeypair.publicKey,
        authority.publicKey
      );

      // 直接开始新游戏
      await gameProgram.methods
        .startGame("Player2")
        .accounts({
          leaderboard: leaderboardKeypair.publicKey,
          gameSession: gameSessionPda,
          payerTokenAccount: payerTokenAccount,
          tokenPool: tokenPool,
          tokenMint: mintKeypair.publicKey,
          payer: authority.publicKey,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      // 等待交易确认
      await new Promise(resolve => setTimeout(resolve, 2000));

      // 2. 提交分数
      const gameSession = await gameProgram.account.gameSession.fetch(gameSessionPda);
      await gameProgram.methods
        .logScore(200, Array.from(gameSession.gameKey))
        .accounts({
          leaderboard: leaderboardKeypair.publicKey,
          gameSession: gameSessionPda,
          payer: authority.publicKey,
        })
        .rpc();

      // 等待交易确认
      await new Promise(resolve => setTimeout(resolve, 2000));

      // 3. 验证排行榜更新
      const leaderboardAccount = await gameProgram.account.leaderboard.fetch(
        leaderboardKeypair.publicKey
      );

      // 验证新分数是否被记录
      const hasScore = leaderboardAccount.players.some(
        player =>
          player.score === 200 &&
          player.name === "Player: Player2"
      );
      expect(hasScore).to.be.true;

      // 验证排序
      const scores = leaderboardAccount.players.map(p => p.score);
      expect(scores).to.deep.equal([...scores].sort((a, b) => b - a));

    } catch (error) {
      console.error("Complete game flow error:", error);
      throw error;
    }
  });
});
