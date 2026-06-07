import {
  CloudDownloadOutlined,
  CloudSyncOutlined,
  CopyOutlined,
  PlayCircleOutlined,
  ReloadOutlined,
  StopOutlined,
  UploadOutlined,
} from "@ant-design/icons";
import {
  Alert,
  Button,
  Card,
  Col,
  Input,
  InputNumber,
  Layout,
  Progress,
  Row,
  Space,
  Statistic,
  Table,
  Tag,
  Tooltip,
  Typography,
  Upload,
  message,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import { useEffect, useMemo, useState } from "react";
import {
  cancelScrape,
  downloadCsv,
  exportCsv,
  initSession,
  listenScrapeProgress,
  parseSkus,
  refreshAll,
  refreshOne,
  runSelfCheck,
  startScrape,
} from "./api";
import "./App.css";
import type { RowResult, ScrapeOptions } from "./types";
import { STATUS_COLORS, STATUS_LABELS } from "./types";

const { Header, Content } = Layout;
const { Title, Text, Paragraph } = Typography;
const { TextArea } = Input;

const DEFAULT_OPTIONS: ScrapeOptions = {
  ratePerSec: 3,
  concurrency: 3,
};

function App() {
  const [inputText, setInputText] = useState("");
  const [rows, setRows] = useState<RowResult[]>([]);
  const [duplicateCount, setDuplicateCount] = useState(0);
  const [options, setOptions] = useState<ScrapeOptions>(DEFAULT_OPTIONS);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState({ done: 0, total: 0 });
  const [sessionMessage, setSessionMessage] = useState("正在初始化会话...");
  const [selfCheckMessage, setSelfCheckMessage] = useState<string | null>(null);
  const [cooldownMessage, setCooldownMessage] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const bootstrap = async () => {
      try {
        const session = await initSession();
        setSessionMessage(session.message);
        const check = await runSelfCheck();
        setSelfCheckMessage(check.ok ? "自检通过" : check.message);
      } catch (error) {
        setSessionMessage(`会话初始化失败: ${String(error)}`);
      }

      unlisten = await listenScrapeProgress((payload) => {
        setProgress({ done: payload.done, total: payload.total });

        if (payload.phase === "Cooling") {
          const batchIndex = payload.batchIndex ?? 0;
          const batchTotal = payload.batchTotal ?? 0;
          const secs = payload.cooldownSecs ?? 30;
          setCooldownMessage(
            `批次 ${batchIndex}/${batchTotal} 完成，冷却 ${secs}s…`,
          );
          setLogs((current) => [
            `[${new Date().toLocaleTimeString()}] 批次冷却 ${secs}s（${batchIndex}/${batchTotal}）`,
            ...current.slice(0, 49),
          ]);
          return;
        }

        setCooldownMessage(null);
        setRows((current) =>
          current.map((row) =>
            row.asin === payload.row.asin ? payload.row : row,
          ),
        );
        setLogs((current) => [
          `[${new Date().toLocaleTimeString()}] ${payload.row.asin} -> ${STATUS_LABELS[payload.row.status]} ${payload.row.priceText ?? ""}`,
          ...current.slice(0, 49),
        ]);
      });
    };

    void bootstrap();
    return () => {
      unlisten?.();
    };
  }, []);

  const validCount = useMemo(
    () => rows.filter((row) => row.status !== "FormatError").length,
    [rows],
  );

  const successCount = useMemo(
    () => rows.filter((row) => row.status === "Success").length,
    [rows],
  );

  const failedCount = useMemo(
    () =>
      rows.filter((row) =>
        ["Failed", "Unavailable", "NoPrice", "Mismatch"].includes(row.status),
      ).length,
    [rows],
  );

  const handleParse = async () => {
    const [parsedRows, duplicates] = await parseSkus(inputText);
    setRows(parsedRows);
    setDuplicateCount(duplicates);
    setProgress({ done: 0, total: parsedRows.length });
    message.success(`识别到 ${parsedRows.length} 条 SKU${duplicates ? `，去重 ${duplicates} 条` : ""}`);
  };

  const handleUpload = async (file: File) => {
    const text = await file.text();
    setInputText(text);
    const [parsedRows, duplicates] = await parseSkus(text);
    setRows(parsedRows);
    setDuplicateCount(duplicates);
    setProgress({ done: 0, total: parsedRows.length });
    message.success(`已从文件读取 ${parsedRows.length} 条 SKU`);
    return false;
  };

  const handleStart = async () => {
    if (rows.length === 0) {
      message.warning("请先输入或上传 SKU");
      return;
    }

    setRunning(true);
    setProgress({ done: 0, total: rows.length });
    try {
      const session = await initSession();
      setSessionMessage(session.message);
      const result = await startScrape(rows, options);
      setRows(result);
      message.success("抓取完成");
    } catch (error) {
      message.error(`抓取失败: ${String(error)}`);
    } finally {
      setRunning(false);
    }
  };

  const handleCancel = async () => {
    await cancelScrape();
    setRunning(false);
    message.info("已请求取消");
  };

  const handleRefreshAll = async () => {
    if (rows.length === 0) {
      return;
    }
    setRunning(true);
    try {
      const result = await refreshAll(options);
      setRows(result);
      message.success("全部刷新完成");
    } catch (error) {
      message.error(String(error));
    } finally {
      setRunning(false);
    }
  };

  const handleExport = async () => {
    if (rows.length === 0) {
      message.warning("没有可导出的数据");
      return;
    }
    const csv = await exportCsv(rows);
    downloadCsv(csv);
    message.success("CSV 已导出");
  };

  const copyPriceValue = async (record: RowResult) => {
    let value = "";
    if (record.priceText && /\./.test(record.priceText)) {
      value = record.priceText.replace(/[^\d.]/g, "");
    } else if (record.priceValue != null) {
      value = record.priceValue.toString();
    } else if (record.priceText) {
      value = record.priceText.replace(/[^\d.]/g, "");
    }
    if (!value) {
      message.warning("暂无可复制的价格");
      return;
    }
    try {
      await navigator.clipboard.writeText(value);
      message.success(`已复制 ${value}`);
    } catch {
      message.error("复制失败");
    }
  };

  const columns: ColumnsType<RowResult> = [
    { title: "SKU", dataIndex: "sku", key: "sku", width: 180, ellipsis: true },
    { title: "dp code", dataIndex: "dpCode", key: "dpCode", width: 120 },
    { title: "ASIN", dataIndex: "asin", key: "asin", width: 120 },
    {
      title: "价格 (JPY)",
      dataIndex: "priceText",
      key: "priceText",
      width: 140,
      render: (value, record) => {
        if (!value) {
          return "-";
        }
        return (
          <Space size={4}>
            <span>{value}</span>
            <Tooltip title="复制数值">
              <Button
                type="text"
                size="small"
                icon={<CopyOutlined />}
                className="copy-price-btn"
                onClick={() => void copyPriceValue(record)}
              />
            </Tooltip>
          </Space>
        );
      },
    },
    {
      title: "Amazon 链接",
      dataIndex: "amazonUrl",
      key: "amazonUrl",
      width: 120,
      render: (value: string) => (
        <a href={value} target="_blank" rel="noreferrer">
          打开
        </a>
      ),
    },
    {
      title: "状态",
      dataIndex: "status",
      key: "status",
      width: 110,
      render: (status: RowResult["status"]) => (
        <Tag color={STATUS_COLORS[status]}>{STATUS_LABELS[status]}</Tag>
      ),
    },
    {
      title: "抓取时间",
      dataIndex: "fetchedAt",
      key: "fetchedAt",
      width: 180,
      render: (value) => value ?? "-",
    },
    {
      title: "操作",
      key: "actions",
      fixed: "right",
      width: 90,
      render: (_, record) => (
        <Button
          size="small"
          icon={<ReloadOutlined />}
          disabled={running || record.status === "FormatError"}
          onClick={async () => {
            setRunning(true);
            try {
              const updated = await refreshOne(record, options);
              setRows((current) =>
                current.map((row) => (row.asin === updated.asin ? updated : row)),
              );
            } catch (error) {
              message.error(String(error));
            } finally {
              setRunning(false);
            }
          }}
        >
          刷新
        </Button>
      ),
    },
  ];

  return (
    <Layout className="app-layout">
      <Header className="app-header">
        <div>
          <Title level={3} style={{ color: "#fff", margin: 0 }}>
            Amazon 价格抓取
          </Title>
          <Text style={{ color: "rgba(255,255,255,0.75)" }}>
            批量解析 SKU，抓取 Amazon.co.jp 商品页 buybox 现价
          </Text>
        </div>
      </Header>

      <Content className="app-content">
        <Space direction="vertical" size="large" style={{ width: "100%" }}>
          <Alert
            className="session-alert"
            type={selfCheckMessage?.includes("通过") ? "success" : "info"}
            showIcon
            message={
              cooldownMessage ??
              (selfCheckMessage
                ? `${sessionMessage} · ${selfCheckMessage}`
                : sessionMessage)
            }
          />

          <Row gutter={[16, 16]}>
            <Col xs={24} xl={10}>
              <Card title="SKU 输入" className="panel-card">
                <Space direction="vertical" style={{ width: "100%" }} size="middle">
                  <TextArea
                    rows={10}
                    value={inputText}
                    placeholder={"每行一个 SKU，例如:\ngx-b0dfxqwpps149"}
                    onChange={(e) => setInputText(e.target.value)}
                  />
                  <Space wrap>
                    <Upload beforeUpload={handleUpload} showUploadList={false} accept=".txt">
                      <Button icon={<UploadOutlined />}>上传 txt</Button>
                    </Upload>
                    <Button
                      type="primary"
                      className="parse-sku-btn"
                      onClick={() => void handleParse()}
                    >
                      解析 SKU
                    </Button>
                  </Space>
                  <Text type="secondary">
                    上传 txt 会自动解析；手动粘贴后请点击「解析 SKU」
                  </Text>
                  <Text type="secondary">
                    已识别 {rows.length} 条，有效 {validCount} 条
                    {duplicateCount > 0 ? `，去重 ${duplicateCount} 条` : ""}
                  </Text>
                </Space>
              </Card>
            </Col>

            <Col xs={24} xl={14}>
              <Card title="抓取控制" className="panel-card">
                <Row gutter={[16, 16]}>
                  <Col xs={12} md={12}>
                    <Text>速率 (条/秒)</Text>
                    <InputNumber
                      min={1}
                      max={3}
                      style={{ width: "100%" }}
                      value={options.ratePerSec}
                      onChange={(value) =>
                        setOptions((current) => ({
                          ...current,
                          ratePerSec: Number(value ?? 3),
                        }))
                      }
                    />
                  </Col>
                  <Col xs={12} md={12}>
                    <Text>并发数</Text>
                    <InputNumber
                      min={1}
                      max={3}
                      style={{ width: "100%" }}
                      value={options.concurrency}
                      onChange={(value) =>
                        setOptions((current) => ({
                          ...current,
                          concurrency: Number(value ?? 3),
                        }))
                      }
                    />
                  </Col>
                </Row>

                <Space wrap style={{ marginTop: 16 }}>
                  <Button
                    type="primary"
                    icon={<PlayCircleOutlined />}
                    loading={running}
                    onClick={() => void handleStart()}
                  >
                    开始抓取
                  </Button>
                  <Button icon={<StopOutlined />} disabled={!running} onClick={() => void handleCancel()}>
                    取消
                  </Button>
                  <Button
                    icon={<CloudSyncOutlined />}
                    disabled={running || rows.length === 0}
                    onClick={() => void handleRefreshAll()}
                  >
                    全部刷新
                  </Button>
                  <Button
                    icon={<CloudDownloadOutlined />}
                    disabled={rows.length === 0}
                    onClick={() => void handleExport()}
                  >
                    导出 CSV
                  </Button>
                </Space>

                <div style={{ marginTop: 20 }}>
                  <Progress
                    percent={
                      progress.total > 0
                        ? Math.round((progress.done / progress.total) * 100)
                        : 0
                    }
                    status={running ? "active" : "normal"}
                  />
                  <Row gutter={16} style={{ marginTop: 12 }}>
                    <Col span={8}>
                      <Statistic title="完成" value={progress.done} suffix={`/ ${progress.total}`} />
                    </Col>
                    <Col span={8}>
                      <Statistic title="成功" value={successCount} />
                    </Col>
                    <Col span={8}>
                      <Statistic title="异常" value={failedCount} />
                    </Col>
                  </Row>
                </div>
              </Card>
            </Col>
          </Row>

          <Card title="结果列表" className="panel-card">
            <Table
              rowKey="asin"
              columns={columns}
              dataSource={rows}
              pagination={{ pageSize: 10, showSizeChanger: true }}
              scroll={{ x: 1100 }}
              size="middle"
            />
          </Card>

          <Card title="实时日志" className="panel-card">
            {logs.length === 0 ? (
              <Paragraph type="secondary">开始抓取后会在这里显示进度日志。</Paragraph>
            ) : (
              logs.map((line, index) => <div key={`${line}-${index}`}>{line}</div>)
            )}
          </Card>
        </Space>
      </Content>
    </Layout>
  );
}

export default App;
