import {Img, staticFile} from 'remotion';

type LogoProps = {
  width: number;
};

export const Logo = ({width}: LogoProps) => (
  <Img
    src={staticFile('dirgo-wordmark-rounded.png')}
    style={{
      width,
      height: 'auto',
      display: 'block',
    }}
  />
);
